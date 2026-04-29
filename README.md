# Kindle 传书

Languages: [中文](#kindle-传书) | [English](#english)

一个基于 Rust、Tauri、React 和 TypeScript 的 Kindle USB 传书桌面工具。当前版本聚焦稳定 USB 直传：自动识别已挂载的 Kindle，上传电子书到 `Kindle/documents/`，并支持读取、删除和重命名设备内书籍。

## 功能

- USB Kindle 自动检测，支持通过卷标和 `documents/system` 目录识别设备。
- 电子书队列上传，支持拖拽和系统文件选择器。
- 相同书籍重复传入时覆盖队列里的旧任务，避免重复显示。
- 上传完成后校验目标文件，并在 UI 中显示写入路径。
- Kindle 书库读取、删除和重命名。
- EPUB 目录修复和原生转换流程。
- 中文、英文、韩文、日文界面切换，默认中文。

## 技术栈

- Desktop: Tauri 2 + Rust
- Frontend: React 18 + TypeScript + TailwindCSS
- Runtime bridge: Tauri commands and events
- Ebook pipeline: Rust native EPUB/TOC/conversion modules

## 本地开发

环境要求：

- Node.js 20+
- Rust stable 1.85+
- macOS / Windows / Linux 桌面环境

安装依赖：

```bash
npm install
```

启动桌面开发模式：

```bash
npm run tauri dev
```

仅构建前端：

```bash
npm run build
```

构建 macOS DMG：

```bash
npm run tauri -- build --target universal-apple-darwin --bundles dmg
```

## 项目结构

```text
src/
  components/        React 通用组件
  data/              浏览器预览模式 mock 数据
  hooks/             前端状态和桌面桥接 hook
  i18n/              多语言文案
  lib/               Tauri API 封装
  pages/             页面组件
  types/             前端类型定义

src-tauri/
  src/converter/     电子书转换
  src/desktop.rs     Tauri 命令和桌面状态桥接
  src/device/        Kindle USB 检测
  src/library/       Kindle 设备内书库管理
  src/toc/           EPUB TOC 修复
  src/uploader/      USB 上传和缩略图处理
```

## 说明

当前版本只保留 USB 传书，不包含局域网传书和邮箱传书。macOS 未公证安装包在其它设备首次打开时，可能需要右键选择“打开”或在“隐私与安全性”中允许打开。

---

# English

Languages: [中文](#kindle-传书) | [English](#english)

Kindle Transfer is a USB-focused desktop app for sending ebooks to Kindle devices. It is built with Rust, Tauri, React, and TypeScript. The current release focuses on stable USB delivery: it detects mounted Kindle devices, uploads ebooks to `Kindle/documents/`, and supports browsing, deleting, and renaming books on the device.

## Features

- Automatic USB Kindle detection by volume label and `documents/system` directory checks.
- Ebook upload queue with drag-and-drop and native file picker support.
- Duplicate book imports replace the existing queued item instead of creating repeated tasks.
- Post-upload destination verification with the final written path shown in the UI.
- On-device Kindle library browsing, deletion, and renaming.
- EPUB table-of-contents repair and native conversion pipeline.
- UI language switching for Chinese, English, Korean, and Japanese. Chinese is the default language.

## Tech Stack

- Desktop: Tauri 2 + Rust
- Frontend: React 18 + TypeScript + TailwindCSS
- Runtime bridge: Tauri commands and events
- Ebook pipeline: native Rust EPUB, TOC, and conversion modules

## Local Development

Requirements:

- Node.js 20+
- Rust stable 1.85+
- macOS / Windows / Linux desktop environment

Install dependencies:

```bash
npm install
```

Start the desktop development app:

```bash
npm run tauri dev
```

Build the frontend only:

```bash
npm run build
```

Build a macOS DMG:

```bash
npm run tauri -- build --target universal-apple-darwin --bundles dmg
```

## Project Structure

```text
src/
  components/        Shared React components
  data/              Mock data for browser preview mode
  hooks/             Frontend state and desktop bridge hooks
  i18n/              Localized UI messages
  lib/               Tauri API wrappers
  pages/             Page components
  types/             Frontend type definitions

src-tauri/
  src/converter/     Ebook conversion
  src/desktop.rs     Tauri commands and desktop state bridge
  src/device/        Kindle USB detection
  src/library/       Kindle on-device library management
  src/toc/           EPUB TOC repair
  src/uploader/      USB upload and thumbnail handling
```

## Notes

The current release only supports USB transfer. LAN transfer and email transfer are not included. The macOS build is not Apple-notarized, so first launch on another Mac may require right-clicking the app and choosing Open, or manually allowing it in Privacy & Security.
