//! Cross-platform Kindle USB detection.
//!
//! This module combines three independent signals to identify a Kindle:
//!
//! - USB bus enumeration via `rusb`
//! - Mount-point change notifications via `notify`
//! - Mount filesystem probing via directory / volume-label heuristics
//!
//! The design intentionally does not trust any single source:
//!
//! - A USB hotplug event may fire before the storage volume is mounted.
//! - A mounted volume may exist even when the USB descriptor is inaccessible.
//! - On some platforms hotplug support is unavailable, so polling is retained as
//!   a fallback safety net.
//!
//! The public API stays small:
//!
//! - [`UsbDetector::scan_now`] performs a synchronous snapshot scan.
//! - [`UsbDetector::start_watch`] starts a background worker and returns a
//!   [`UsbWatchHandle`] for consuming events and stopping the watcher.

use notify::{recommended_watcher, RecommendedWatcher, RecursiveMode, Watcher};
use rusb::{Context, Device, DeviceDescriptor, Hotplug, HotplugBuilder, UsbContext};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvError, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{debug, warn};

const AMAZON_VENDOR_ID: u16 = 0x1949;
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(3);
const DEFAULT_DEBOUNCE_WINDOW: Duration = Duration::from_millis(400);
const USB_EVENT_TIMEOUT: Duration = Duration::from_millis(50);

/// Normalized Kindle device information returned to the rest of the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindleDevice {
    /// Stable identifier. Prefer the device serial when available, otherwise
    /// fall back to the normalized mount path.
    pub id: String,
    /// Human readable name shown to the user.
    pub name: String,
    /// Mounted filesystem root of the Kindle volume.
    pub mount_path: String,
    /// USB serial number when readable.
    pub serial: Option<String>,
    /// Best-effort model hint. This usually comes from the USB product string.
    pub model: Option<String>,
    /// Best-effort firmware hint. This detector leaves it as `None` unless a
    /// known metadata file is found on the mounted filesystem.
    pub firmware: Option<String>,
}

/// Runtime configuration for USB detection.
#[derive(Debug, Clone)]
pub struct UsbDetectorConfig {
    /// Periodic fallback scan interval. This keeps detection working even when
    /// native hotplug events or mount notifications are missed.
    pub poll_interval: Duration,
    /// Minimum delay between consecutive full rescans.
    pub debounce_window: Duration,
    /// Parent directories that may receive Kindle mount points.
    pub mount_roots: Vec<PathBuf>,
}

impl Default for UsbDetectorConfig {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL_INTERVAL,
            debounce_window: DEFAULT_DEBOUNCE_WINDOW,
            mount_roots: default_mount_roots(),
        }
    }
}

/// Events emitted by a running watcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsbWatchEvent {
    /// Complete state snapshot after an initial scan or a topology change.
    Snapshot(Vec<KindleDevice>),
    /// Newly connected Kindle.
    Connected(KindleDevice),
    /// Existing Kindle metadata changed, typically because USB details became
    /// available after the filesystem was already mounted.
    Updated(KindleDevice),
    /// Kindle removed or no longer mounted.
    Disconnected(KindleDevice),
    /// Non-fatal background warning. The watcher keeps running.
    Error(String),
}

/// Public error type for setup and synchronous scans.
#[derive(Debug, Error)]
pub enum UsbDetectorError {
    #[error("USB subsystem initialization failed: {0}")]
    Usb(#[from] rusb::Error),
    #[error("filesystem watcher initialization failed: {0}")]
    Notify(#[from] notify::Error),
    #[error("filesystem operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("watcher thread unexpectedly terminated")]
    WorkerStopped,
    #[error("device scan failed: {0}")]
    Scan(String),
    #[error("failed to stop watcher thread")]
    JoinFailed,
}

/// Main entrypoint for Kindle USB detection.
#[derive(Debug, Clone)]
pub struct UsbDetector {
    config: UsbDetectorConfig,
}

impl Default for UsbDetector {
    fn default() -> Self {
        Self::new(UsbDetectorConfig::default())
    }
}

impl UsbDetector {
    /// Create a detector with an explicit runtime configuration.
    pub fn new(config: UsbDetectorConfig) -> Self {
        Self { config }
    }

    /// Perform a one-shot synchronous scan.
    ///
    /// This method is useful for bootstrapping UI state before a watcher is
    /// started, or for deterministic tests.
    pub fn scan_now(&self) -> Result<Vec<KindleDevice>, UsbDetectorError> {
        let report = scan_devices(&self.config);

        if report.devices.is_empty() && !report.warnings.is_empty() {
            return Err(UsbDetectorError::Scan(report.warnings.join("; ")));
        }

        Ok(report.devices)
    }

    /// Start the background watcher.
    ///
    /// The worker performs an initial snapshot scan, then reacts to:
    ///
    /// - libusb hotplug callbacks when supported
    /// - filesystem notifications on known mount roots
    /// - periodic fallback polling
    pub fn start_watch(&self) -> Result<UsbWatchHandle, UsbDetectorError> {
        let (event_tx, event_rx) = mpsc::channel();
        let (command_tx, command_rx) = mpsc::channel();
        let config = self.config.clone();
        let worker_command_tx = command_tx.clone();

        let worker = thread::Builder::new()
            .name("kindle-usb-detector".to_string())
            .spawn(move || watch_worker(config, command_rx, worker_command_tx, event_tx))
            .map_err(|err| {
                UsbDetectorError::Scan(format!("failed to spawn watcher thread: {err}"))
            })?;

        Ok(UsbWatchHandle {
            event_rx,
            command_tx,
            worker: Some(worker),
        })
    }
}

/// Handle for a running USB watcher.
///
/// The handle owns the watcher thread lifecycle and exposes a small blocking
/// event consumption API so the caller can bridge the detector into a Tauri
/// event emitter or into a higher-level device manager.
#[derive(Debug)]
pub struct UsbWatchHandle {
    event_rx: Receiver<UsbWatchEvent>,
    command_tx: Sender<WorkerCommand>,
    worker: Option<JoinHandle<()>>,
}

impl UsbWatchHandle {
    /// Block until the next event is available.
    pub fn recv(&self) -> Result<UsbWatchEvent, RecvError> {
        self.event_rx.recv()
    }

    /// Block until the next event or timeout.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<UsbWatchEvent, RecvTimeoutError> {
        self.event_rx.recv_timeout(timeout)
    }

    /// Gracefully stop the watcher thread.
    pub fn stop(&mut self) -> Result<(), UsbDetectorError> {
        let _ = self.command_tx.send(WorkerCommand::Shutdown);

        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| UsbDetectorError::JoinFailed)?;
        }

        Ok(())
    }
}

impl Drop for UsbWatchHandle {
    fn drop(&mut self) {
        let _ = self.command_tx.send(WorkerCommand::Shutdown);

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Debug)]
enum WorkerCommand {
    Trigger(TriggerSource),
    Shutdown,
}

#[derive(Debug, Clone, Copy)]
enum TriggerSource {
    Startup,
    UsbHotplug,
    Filesystem,
    Poll,
}

#[derive(Debug)]
struct ScanReport {
    devices: Vec<KindleDevice>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct UsbCandidate {
    product_name: Option<String>,
    serial: Option<String>,
    bus_number: u8,
    address: u8,
}

#[derive(Debug, Clone)]
struct MountCandidate {
    mount_path: PathBuf,
    volume_label: Option<String>,
}

fn watch_worker(
    config: UsbDetectorConfig,
    command_rx: Receiver<WorkerCommand>,
    command_tx: Sender<WorkerCommand>,
    event_tx: Sender<UsbWatchEvent>,
) {
    let watch_setup = WatchRuntime::new(&config, command_rx, command_tx, event_tx.clone());

    let mut runtime = match watch_setup {
        Ok(runtime) => runtime,
        Err(err) => {
            let _ = event_tx.send(UsbWatchEvent::Error(err.to_string()));
            return;
        }
    };

    runtime.run();
}

struct WatchRuntime {
    config: UsbDetectorConfig,
    command_rx: Receiver<WorkerCommand>,
    event_tx: Sender<UsbWatchEvent>,
    watchers: Vec<RecommendedWatcher>,
    usb_context: Option<Context>,
    usb_registration: Option<rusb::Registration<Context>>,
    last_state: BTreeMap<String, KindleDevice>,
    last_scan_at: Option<Instant>,
}

impl WatchRuntime {
    fn new(
        config: &UsbDetectorConfig,
        command_rx: Receiver<WorkerCommand>,
        command_tx: Sender<WorkerCommand>,
        event_tx: Sender<UsbWatchEvent>,
    ) -> Result<Self, UsbDetectorError> {
        let watchers = build_mount_watchers(config, &command_tx, &event_tx)?;
        let (usb_context, usb_registration) = build_usb_hotplug_registration(&command_tx)?;

        Ok(Self {
            config: config.clone(),
            command_rx,
            event_tx,
            watchers,
            usb_context,
            usb_registration,
            last_state: BTreeMap::new(),
            last_scan_at: None,
        })
    }

    fn run(&mut self) {
        if let Err(err) = self.handle_trigger(TriggerSource::Startup) {
            let _ = self.event_tx.send(UsbWatchEvent::Error(err.to_string()));
        }

        loop {
            if let Some(context) = &self.usb_context {
                if let Err(err) = context.handle_events(Some(USB_EVENT_TIMEOUT)) {
                    let _ = self
                        .event_tx
                        .send(UsbWatchEvent::Error(format!("USB event loop error: {err}")));
                }
            }

            match self.command_rx.recv_timeout(Duration::from_millis(200)) {
                Ok(WorkerCommand::Shutdown) => break,
                Ok(WorkerCommand::Trigger(source)) => {
                    if let Err(err) = self.handle_trigger(source) {
                        let _ = self.event_tx.send(UsbWatchEvent::Error(err.to_string()));
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.should_poll() {
                        if let Err(err) = self.handle_trigger(TriggerSource::Poll) {
                            let _ = self.event_tx.send(UsbWatchEvent::Error(err.to_string()));
                        }
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        if let Some(context) = &self.usb_context {
            context.interrupt_handle_events();
        }

        debug!(
            watcher_count = self.watchers.len(),
            hotplug_enabled = self.usb_registration.is_some(),
            "USB detector worker stopped",
        );
    }

    fn should_poll(&self) -> bool {
        match self.last_scan_at {
            Some(last_scan_at) => last_scan_at.elapsed() >= self.config.poll_interval,
            None => true,
        }
    }

    fn handle_trigger(&mut self, source: TriggerSource) -> Result<(), UsbDetectorError> {
        if let Some(last_scan_at) = self.last_scan_at {
            let elapsed = last_scan_at.elapsed();
            if elapsed < self.config.debounce_window {
                thread::sleep(self.config.debounce_window - elapsed);
            }
        }

        debug!(?source, "starting Kindle USB scan");

        let report = scan_devices(&self.config);

        for warning in report.warnings {
            let _ = self.event_tx.send(UsbWatchEvent::Error(warning));
        }

        self.last_scan_at = Some(Instant::now());
        emit_state_diff(&mut self.last_state, report.devices, &self.event_tx);

        Ok(())
    }
}

fn build_mount_watchers(
    config: &UsbDetectorConfig,
    command_tx: &Sender<WorkerCommand>,
    event_tx: &Sender<UsbWatchEvent>,
) -> Result<Vec<RecommendedWatcher>, UsbDetectorError> {
    let mut watchers = Vec::new();
    let watch_roots = discover_mount_watch_roots(config)?;

    for root in watch_roots {
        if !root.exists() {
            debug!(path = %root.display(), "mount watch root does not exist, skipping");
            continue;
        }

        let command_sender = command_tx.clone();
        let event_sender = event_tx.clone();
        let mut watcher =
            recommended_watcher(move |result: notify::Result<notify::Event>| match result {
                Ok(event) => {
                    debug!(paths = ?event.paths, "mount activity detected");
                    let _ = command_sender.send(WorkerCommand::Trigger(TriggerSource::Filesystem));
                }
                Err(err) => {
                    let _ = event_sender.send(UsbWatchEvent::Error(format!(
                        "filesystem watcher error: {err}"
                    )));
                }
            })?;

        watcher.watch(&root, mount_watch_mode())?;
        watchers.push(watcher);
    }

    Ok(watchers)
}

fn build_usb_hotplug_registration(
    command_tx: &Sender<WorkerCommand>,
) -> Result<(Option<Context>, Option<rusb::Registration<Context>>), UsbDetectorError> {
    if !rusb::has_hotplug() {
        debug!("libusb hotplug support is unavailable, falling back to polling");
        return Ok((None, None));
    }

    let context = Context::new()?;
    let callback = Box::new(UsbHotplugCallback {
        command_tx: command_tx.clone(),
    });

    let mut builder = HotplugBuilder::new();
    builder.vendor_id(AMAZON_VENDOR_ID).enumerate(true);
    let registration = builder.register(context.clone(), callback)?;

    Ok((Some(context), Some(registration)))
}

struct UsbHotplugCallback {
    command_tx: Sender<WorkerCommand>,
}

impl Hotplug<Context> for UsbHotplugCallback {
    fn device_arrived(&mut self, _device: Device<Context>) {
        let _ = self
            .command_tx
            .send(WorkerCommand::Trigger(TriggerSource::UsbHotplug));
    }

    fn device_left(&mut self, _device: Device<Context>) {
        let _ = self
            .command_tx
            .send(WorkerCommand::Trigger(TriggerSource::UsbHotplug));
    }
}

fn emit_state_diff(
    last_state: &mut BTreeMap<String, KindleDevice>,
    devices: Vec<KindleDevice>,
    event_tx: &Sender<UsbWatchEvent>,
) {
    let next_state: BTreeMap<String, KindleDevice> = devices
        .into_iter()
        .map(|device| (device.id.clone(), device))
        .collect();

    for (id, old_device) in last_state.iter() {
        if !next_state.contains_key(id) {
            let _ = event_tx.send(UsbWatchEvent::Disconnected(old_device.clone()));
        }
    }

    for (id, new_device) in &next_state {
        match last_state.get(id) {
            None => {
                let _ = event_tx.send(UsbWatchEvent::Connected(new_device.clone()));
            }
            Some(old_device) if old_device != new_device => {
                let _ = event_tx.send(UsbWatchEvent::Updated(new_device.clone()));
            }
            Some(_) => {}
        }
    }

    let snapshot = next_state.values().cloned().collect::<Vec<_>>();
    let _ = event_tx.send(UsbWatchEvent::Snapshot(snapshot));
    *last_state = next_state;
}

fn scan_devices(config: &UsbDetectorConfig) -> ScanReport {
    let mut warnings = Vec::new();

    let usb_candidates = match scan_usb_candidates() {
        Ok(devices) => devices,
        Err(err) => {
            warnings.push(format!("USB enumeration failed: {err}"));
            Vec::new()
        }
    };

    let mount_candidates = match scan_mount_candidates(config) {
        Ok(devices) => devices,
        Err(err) => {
            warnings.push(format!("mount enumeration failed: {err}"));
            Vec::new()
        }
    };

    let devices = merge_candidates(mount_candidates, usb_candidates);

    ScanReport { devices, warnings }
}

fn scan_usb_candidates() -> Result<Vec<UsbCandidate>, rusb::Error> {
    let devices = rusb::devices()?;
    let mut candidates = Vec::new();

    for device in devices.iter() {
        let descriptor = match device.device_descriptor() {
            Ok(descriptor) => descriptor,
            Err(err) => {
                warn!("failed to read USB descriptor: {err}");
                continue;
            }
        };

        if descriptor.vendor_id() != AMAZON_VENDOR_ID {
            continue;
        }

        candidates.push(UsbCandidate {
            product_name: read_product_name(&device, &descriptor),
            serial: read_serial_number(&device, &descriptor),
            bus_number: device.bus_number(),
            address: device.address(),
        });
    }

    candidates.sort_by_key(|candidate| (candidate.bus_number, candidate.address));

    Ok(candidates)
}

fn read_product_name<T: UsbContext>(
    device: &Device<T>,
    descriptor: &DeviceDescriptor,
) -> Option<String> {
    let handle = device.open().ok()?;
    let value = handle.read_product_string_ascii(descriptor).ok()?;
    normalize_optional_string(Some(value))
}

fn read_serial_number<T: UsbContext>(
    device: &Device<T>,
    descriptor: &DeviceDescriptor,
) -> Option<String> {
    let handle = device.open().ok()?;
    let value = handle.read_serial_number_string_ascii(descriptor).ok()?;
    normalize_optional_string(Some(value))
}

fn scan_mount_candidates(config: &UsbDetectorConfig) -> Result<Vec<MountCandidate>, io::Error> {
    let mount_paths = platform_mount_paths(config)?;
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    for mount_path in mount_paths {
        if !mount_path.is_dir() {
            continue;
        }

        let normalized = normalize_mount_path(&mount_path);
        if !seen.insert(normalized) {
            continue;
        }

        let volume_label = read_volume_label(&mount_path).ok().flatten();
        let path_label_matches = mount_path
            .file_name()
            .and_then(|value| value.to_str())
            .map(is_kindle_label)
            .unwrap_or(false);

        let volume_label_matches = volume_label
            .as_deref()
            .map(is_kindle_label)
            .unwrap_or(false);

        let has_documents = mount_path.join("documents").is_dir();
        let has_system = mount_path.join("system").is_dir();

        if volume_label_matches || path_label_matches || (has_documents && has_system) {
            candidates.push(MountCandidate {
                mount_path,
                volume_label,
            });
        }
    }

    candidates.sort_by(|left, right| left.mount_path.cmp(&right.mount_path));

    Ok(candidates)
}

fn merge_candidates(
    mounts: Vec<MountCandidate>,
    usb_candidates: Vec<UsbCandidate>,
) -> Vec<KindleDevice> {
    let mut devices = mounts
        .into_iter()
        .map(|mount| {
            let mount_path = mount.mount_path.to_string_lossy().into_owned();
            let serial = None;
            let id = build_device_id(serial.as_deref(), &mount_path);
            let firmware = read_firmware_hint(Path::new(&mount_path));
            let display_name = mount
                .volume_label
                .clone()
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| "Kindle".to_string());

            KindleDevice {
                id,
                name: display_name,
                mount_path,
                serial,
                model: None,
                firmware,
            }
        })
        .collect::<Vec<_>>();

    if usb_candidates.is_empty() {
        return devices;
    }

    if usb_candidates.len() == devices.len() {
        for (device, usb) in devices.iter_mut().zip(usb_candidates.iter()) {
            enrich_with_usb(device, usb);
        }

        return devices;
    }

    if usb_candidates.len() == 1 {
        if let Some(device) = devices.first_mut() {
            enrich_with_usb(device, &usb_candidates[0]);
        }
    }

    devices
}

fn enrich_with_usb(device: &mut KindleDevice, usb: &UsbCandidate) {
    if let Some(serial) = usb.serial.clone() {
        device.serial = Some(serial.clone());
        device.id = build_device_id(Some(&serial), &device.mount_path);
    }

    if let Some(product_name) = usb.product_name.clone() {
        device.model = Some(product_name.clone());

        if device.name.eq_ignore_ascii_case("kindle") {
            device.name = product_name;
        }
    }
}

fn build_device_id(serial: Option<&str>, mount_path: &str) -> String {
    match serial {
        Some(serial) => format!("kindle-{}", sanitize_identifier(serial)),
        None => format!("kindle-{}", sanitize_identifier(mount_path)),
    }
}

fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch.to_ascii_lowercase(),
            _ => '-',
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|inner| {
        let trimmed = inner.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_mount_path(path: &Path) -> String {
    let raw = path.to_string_lossy().to_string();

    if cfg!(target_os = "windows") {
        raw.to_ascii_lowercase()
    } else {
        raw
    }
}

fn is_kindle_label(label: &str) -> bool {
    label.trim().eq_ignore_ascii_case("kindle")
}

fn read_firmware_hint(mount_path: &Path) -> Option<String> {
    const KNOWN_HINT_FILES: &[&str] = &["system/version.txt", "system/firmware.txt", "version.txt"];

    for relative in KNOWN_HINT_FILES {
        let path = mount_path.join(relative);
        if let Ok(contents) = fs::read_to_string(path) {
            let first_line = contents.lines().next().map(str::trim).unwrap_or_default();
            if !first_line.is_empty() {
                return Some(first_line.to_string());
            }
        }
    }

    None
}

fn mount_watch_mode() -> RecursiveMode {
    RecursiveMode::NonRecursive
}

fn default_mount_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        Vec::new()
    }

    #[cfg(target_os = "macos")]
    {
        vec![PathBuf::from("/Volumes")]
    }

    #[cfg(target_os = "linux")]
    {
        vec![
            PathBuf::from("/media"),
            PathBuf::from("/run/media"),
            PathBuf::from("/mnt"),
        ]
    }
}

fn platform_mount_paths(config: &UsbDetectorConfig) -> Result<Vec<PathBuf>, io::Error> {
    #[cfg(target_os = "windows")]
    {
        windows_mount_paths()
    }

    #[cfg(target_os = "macos")]
    {
        unix_mount_paths(&config.mount_roots, 1)
    }

    #[cfg(target_os = "linux")]
    {
        linux_mount_paths(config)
    }
}

fn discover_mount_watch_roots(config: &UsbDetectorConfig) -> Result<Vec<PathBuf>, io::Error> {
    #[cfg(target_os = "linux")]
    {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();

        for root in &config.mount_roots {
            let normalized = normalize_mount_path(root);
            if seen.insert(normalized) {
                roots.push(root.clone());
            }

            if !root.is_dir() {
                continue;
            }

            for entry in fs::read_dir(root)? {
                let entry = entry?;
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                let normalized = normalize_mount_path(&path);
                if seen.insert(normalized) {
                    roots.push(path);
                }
            }
        }

        Ok(roots)
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        Ok(config.mount_roots.clone())
    }
}

#[cfg(target_os = "macos")]
fn unix_mount_paths(roots: &[PathBuf], max_depth: usize) -> Result<Vec<PathBuf>, io::Error> {
    let mut mounts = Vec::new();

    for root in roots {
        if !root.is_dir() {
            continue;
        }

        collect_directory_children(root, max_depth, &mut mounts)?;
    }

    Ok(mounts)
}

#[cfg(target_os = "linux")]
fn linux_mount_paths(config: &UsbDetectorConfig) -> Result<Vec<PathBuf>, io::Error> {
    let mounts_file =
        fs::read_to_string("/proc/self/mounts").or_else(|_| fs::read_to_string("/etc/mtab"))?;
    let root_set = config
        .mount_roots
        .iter()
        .map(|path| path.as_path())
        .collect::<Vec<_>>();
    let mut mounts = Vec::new();

    for line in mounts_file.lines() {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 3 {
            continue;
        }

        let mount_point = unescape_mount_field(columns[1]);
        let path = PathBuf::from(mount_point);

        if !path.is_dir() {
            continue;
        }

        if root_set.iter().any(|root| path.starts_with(root)) {
            mounts.push(path);
        }
    }

    Ok(mounts)
}

#[cfg(target_os = "linux")]
fn unescape_mount_field(value: &str) -> String {
    value
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

#[cfg(target_os = "macos")]
fn collect_directory_children(
    root: &Path,
    depth: usize,
    output: &mut Vec<PathBuf>,
) -> Result<(), io::Error> {
    if depth == 0 {
        output.push(root.to_path_buf());
        return Ok(());
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        if depth == 1 {
            output.push(path);
        } else {
            collect_directory_children(&path, depth - 1, output)?;
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_mount_paths() -> Result<Vec<PathBuf>, io::Error> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, DRIVE_FIXED, DRIVE_REMOVABLE,
    };

    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Err(io::Error::last_os_error());
    }

    let mut mounts = Vec::new();

    for letter in b'A'..=b'Z' {
        let bit = 1_u32 << u32::from(letter - b'A');
        if mask & bit == 0 {
            continue;
        }

        let root = format!("{}:\\", letter as char);
        let wide = Path::new(&root)
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>();

        let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
        if drive_type == DRIVE_REMOVABLE || drive_type == DRIVE_FIXED {
            mounts.push(PathBuf::from(root));
        }
    }

    Ok(mounts)
}

fn read_volume_label(path: &Path) -> Result<Option<String>, io::Error> {
    #[cfg(target_os = "windows")]
    {
        read_windows_volume_label(path)
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        Ok(path
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()))
    }
}

#[cfg(target_os = "windows")]
fn read_windows_volume_label(path: &Path) -> Result<Option<String>, io::Error> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    let mut name_buffer = [0_u16; 261];

    unsafe {
        GetVolumeInformationW(
            PCWSTR(wide.as_ptr()),
            Some(&mut name_buffer),
            None,
            None,
            None,
            None,
        )
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
    }

    let length = name_buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(name_buffer.len());
    let label = String::from_utf16_lossy(&name_buffer[..length])
        .trim()
        .to_string();

    if label.is_empty() {
        Ok(None)
    } else {
        Ok(Some(label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kindle_label_match_is_case_insensitive() {
        assert!(is_kindle_label("Kindle"));
        assert!(is_kindle_label("kindle"));
        assert!(is_kindle_label("  KINDLE  "));
        assert!(!is_kindle_label("Paperwhite"));
    }

    #[test]
    fn identifier_prefers_serial_when_available() {
        let id = build_device_id(Some("G090 ABC"), "/Volumes/Kindle");
        assert_eq!(id, "kindle-g090-abc");
    }

    #[test]
    fn identifier_falls_back_to_mount_path() {
        let id = build_device_id(None, "/Volumes/Kindle");
        assert_eq!(id, "kindle-volumes-kindle");
    }
}
