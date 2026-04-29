/**
 * Lightweight i18n layer for the desktop shell.
 *
 * Chinese is the default locale. The app keeps locale state in localStorage and
 * lets React trigger re-rendering from the top-level shell when users switch
 * languages.
 */

import type { DeviceStatus, HistoryStatus, QueueStage } from "../types";

export type Locale = "zh-CN" | "en-US" | "ko-KR" | "ja-JP";

export const DEFAULT_LOCALE: Locale = "zh-CN";
export const LOCALE_OPTIONS: Array<{ value: Locale; label: string }> = [
  { value: "zh-CN", label: "Chinese" },
  { value: "en-US", label: "English" },
  { value: "ko-KR", label: "Korean" },
  { value: "ja-JP", label: "Japanese" },
];

const LOCALE_STORAGE_KEY = "kindle-transfer-locale";

const zhCN = {
  "app.name": "Kindle 传书",
  "app.topbarEyebrow": "Kindle 传书",
  "view.devices": "设备",
  "view.upload": "上传",
  "view.library": "书库",
  "view.history": "历史",
  "view.devicesTitle": "设备概览",
  "view.uploadTitle": "上传流程",
  "view.libraryTitle": "Kindle 书库",
  "view.historyTitle": "传输历史",
  "sidebar.devicesNote": "在线 Kindle 设备",
  "sidebar.uploadNote": "转换并投递",
  "sidebar.libraryNote": "查看与删除",
  "sidebar.historyNote": "传输档案",
  "devices.summary.online": "在线设备",
  "devices.summary.onlineDetail": "当前可达",
  "devices.summary.usbDetail": "直连挂载",
  "devices.summary.lanDetail": "无线通道",
  "devices.emptyTitle": "还没有发现可连接的 Kindle",
  "devices.emptyBody": "请确认 Kindle 已通过 USB 接入，并且系统已经成功挂载设备存储。",
  "devices.lanHintTitle": "USB 提示",
  "devices.lanHintBody": "当前版本只支持 USB 直连传书，请确认设备已经挂载并能看到 documents 目录。",
  "devices.uploadUnavailable": "当前版本仅支持 USB 直传。",
  "device.status.ready": "就绪",
  "device.status.syncing": "同步中",
  "device.status.idle": "空闲",
  "device.metric.battery": "电量",
  "device.metric.storage": "存储",
  "device.metric.mount": "挂载路径",
  "device.metric.address": "地址",
  "device.metric.storageLoad": "存储占用",
  "common.unknown": "未知",
  "upload.currentTarget": "当前目标设备",
  "upload.noDevice": "还没有检测到 Kindle 设备。",
  "upload.deviceUnavailable": "当前版本仅支持 USB 直传。",
  "upload.batchState": "批次状态",
  "upload.transferRunning": "传输进行中",
  "upload.startTransfer": "开始传输",
  "upload.pausePipeline": "暂停流程",
  "upload.overallProgress": "总体进度",
  "upload.targetFormatBias": "目标格式",
  "upload.queueWeight": "队列体积",
  "upload.activeQueue": "当前队列",
  "upload.queueClear": "队列为空",
  "upload.live": "实时",
  "upload.dropToStart": "拖入电子书即可开始新一轮传输。",
  "upload.finishedCount": "{done}/{total} 已完成",
  "upload.processingCount": "{count} 本处理中",
  "upload.localImport": "本地导入",
  "upload.progress.conversion": "转换",
  "upload.progress.upload": "上传",
  "upload.writtenTo": "已写入",
  "upload.stage.done": "完成",
  "upload.stage.failed": "失败",
  "upload.stage.queued": "排队中",
  "upload.stage.locked": "已锁定",
  "upload.stage.running": "进行中",
  "upload.stage.verifying": "校验中",
  "upload.stage.waiting": "等待中",
  "dropzone.tauriTitle": "把电子书拖入桌面窗口",
  "dropzone.browserTitle": "拖入 EPUB、MOBI、AZW3 或 PDF",
  "dropzone.browseButton": "选择文件",
  "dropzone.disabledDescription": "先选择一个 Kindle 设备，队列才能绑定正确的传输目标。",
  "dropzone.tauriDescription": "桌面窗口拖拽会把绝对路径直接交给 Rust 后端，由它负责入队和上传。",
  "dropzone.browserDescription": "流程会识别目标 Kindle，修复目录，转换到合适格式，并通过 USB 完成投递。",
  "dropzone.feature.bulk": "批量入队",
  "dropzone.feature.toc": "目录修复",
  "dropzone.feature.format": "智能格式匹配",
  "dialog.uploadTitle": "选择要传到 Kindle 的电子书",
  "dialog.ebookFilter": "电子书",
  "history.eyebrow": "传输记录",
  "history.title": "检索每一次传书结果。",
  "history.description": "按格式决策和最终结果回看最近的 Kindle 传输会话。",
  "history.metric.total": "总计",
  "history.metric.totalDetail": "记录任务",
  "history.metric.success": "成功",
  "history.metric.successDetail": "无异常完成",
  "history.metric.review": "待复核",
  "history.metric.reviewDetail": "部分成功或失败",
  "history.searchPlaceholder": "搜索书名、设备或格式",
  "history.filter.allStatus": "全部状态",
  "history.filter.success": "成功",
  "history.filter.partial": "部分成功",
  "history.filter.failed": "失败",
  "history.filter.allRoutes": "全部通道",
  "history.table.book": "书籍",
  "history.table.target": "目标设备",
  "history.table.output": "输出格式",
  "history.table.finished": "完成时间",
  "history.table.status": "状态",
  "history.status.success": "成功",
  "history.status.partial": "部分成功",
  "history.status.failed": "失败",
  "history.empty": "暂无传输记录",
  "library.currentDevice": "当前设备",
  "library.noDevice": "未检测到 Kindle",
  "library.refresh": "刷新书库",
  "library.refreshing": "刷新中",
  "library.bookList": "设备内书籍",
  "library.booksUnit": "本书",
  "library.loading": "正在读取 Kindle 书库。",
  "library.empty": "这台 Kindle 的 documents 目录里没有可识别的书籍。",
  "library.rename": "重命名",
  "library.renaming": "保存中",
  "library.renameSave": "保存",
  "library.renameCancel": "取消",
  "library.renamePlaceholder": "输入新的书名",
  "library.renameEmpty": "新书名不能为空。",
  "library.delete": "删除",
  "library.deleting": "删除中",
  "library.deleteConfirm": "确定要从 Kindle 删除《{title}》吗？",
} as const;

type MessageKey = keyof typeof zhCN;
type MessageCatalog = Record<MessageKey, string>;

const enUS: MessageCatalog = {
  "app.name": "Kindle Relay",
  "app.topbarEyebrow": "Kindle Transfer Desk",
  "view.devices": "Devices",
  "view.upload": "Upload",
  "view.library": "Library",
  "view.history": "History",
  "view.devicesTitle": "Device Overview",
  "view.uploadTitle": "Upload Pipeline",
  "view.libraryTitle": "Kindle Library",
  "view.historyTitle": "Transfer History",
  "sidebar.devicesNote": "Live Kindle fleet",
  "sidebar.uploadNote": "Convert and deliver",
  "sidebar.libraryNote": "Browse and delete",
  "sidebar.historyNote": "Transfer archive",
  "devices.summary.online": "Online",
  "devices.summary.onlineDetail": "Active now",
  "devices.summary.usbDetail": "Direct mount",
  "devices.summary.lanDetail": "Wireless route",
  "devices.emptyTitle": "No reachable Kindle devices yet",
  "devices.emptyBody": "Confirm the Kindle is connected over USB and that the operating system mounted its storage volume.",
  "devices.lanHintTitle": "USB note",
  "devices.lanHintBody": "The current build supports USB transfer only, so confirm the Kindle is mounted and exposes the documents directory.",
  "devices.uploadUnavailable": "The current build supports USB transfer only.",
  "device.status.ready": "ready",
  "device.status.syncing": "syncing",
  "device.status.idle": "idle",
  "device.metric.battery": "Battery",
  "device.metric.storage": "Storage",
  "device.metric.mount": "Mount",
  "device.metric.address": "Address",
  "device.metric.storageLoad": "Storage load",
  "common.unknown": "Unknown",
  "upload.currentTarget": "Current target",
  "upload.noDevice": "No Kindle detected yet.",
  "upload.deviceUnavailable": "The current build supports USB transfer only.",
  "upload.batchState": "Batch state",
  "upload.transferRunning": "Transfer Running",
  "upload.startTransfer": "Start Transfer",
  "upload.pausePipeline": "Pause Pipeline",
  "upload.overallProgress": "Overall Progress",
  "upload.targetFormatBias": "Target Format",
  "upload.queueWeight": "Queue Weight",
  "upload.activeQueue": "Active Queue",
  "upload.queueClear": "Queue is clear",
  "upload.live": "Live",
  "upload.dropToStart": "Drop a book to start a delivery batch.",
  "upload.finishedCount": "{done}/{total} completed",
  "upload.processingCount": "{count} processing",
  "upload.localImport": "Local Import",
  "upload.progress.conversion": "Conversion",
  "upload.progress.upload": "Upload",
  "upload.writtenTo": "Written to",
  "upload.stage.done": "Done",
  "upload.stage.failed": "Failed",
  "upload.stage.queued": "Queued",
  "upload.stage.locked": "Locked",
  "upload.stage.running": "Running",
  "upload.stage.verifying": "Verifying",
  "upload.stage.waiting": "Waiting",
  "dropzone.tauriTitle": "Drop books into the desktop window",
  "dropzone.browserTitle": "Drop EPUB, MOBI, AZW3 or PDF here",
  "dropzone.browseButton": "Choose Files",
  "dropzone.disabledDescription": "Select a Kindle device first so the queue can target the right delivery route.",
  "dropzone.tauriDescription": "Native window drop sends absolute file paths to the Rust backend, which can queue and upload them directly.",
  "dropzone.browserDescription": "The pipeline will inspect the target Kindle, normalize the table of contents, convert to the best format, then deliver over USB.",
  "dropzone.feature.bulk": "Bulk Queue",
  "dropzone.feature.toc": "TOC Repair",
  "dropzone.feature.format": "Smart Format Match",
  "dialog.uploadTitle": "Choose books to send to Kindle",
  "dialog.ebookFilter": "Ebooks",
  "history.eyebrow": "Transfer Archive",
  "history.title": "Search every delivery result.",
  "history.description": "Trace format decisions and delivery outcomes across your recent Kindle sessions.",
  "history.metric.total": "Total",
  "history.metric.totalDetail": "Recorded jobs",
  "history.metric.success": "Success",
  "history.metric.successDetail": "Clean transfers",
  "history.metric.review": "Needs Review",
  "history.metric.reviewDetail": "Partial or failed",
  "history.searchPlaceholder": "Search title, device or format",
  "history.filter.allStatus": "All status",
  "history.filter.success": "Success",
  "history.filter.partial": "Partial",
  "history.filter.failed": "Failed",
  "history.filter.allRoutes": "All routes",
  "history.table.book": "Book",
  "history.table.target": "Target",
  "history.table.output": "Output",
  "history.table.finished": "Finished",
  "history.table.status": "Status",
  "history.status.success": "success",
  "history.status.partial": "partial",
  "history.status.failed": "failed",
  "history.empty": "No transfer records yet.",
  "library.currentDevice": "Current device",
  "library.noDevice": "No Kindle detected",
  "library.refresh": "Refresh Library",
  "library.refreshing": "Refreshing",
  "library.bookList": "On-device books",
  "library.booksUnit": "books",
  "library.loading": "Reading the Kindle library.",
  "library.empty": "No readable books were found in this Kindle documents folder.",
  "library.rename": "Rename",
  "library.renaming": "Saving",
  "library.renameSave": "Save",
  "library.renameCancel": "Cancel",
  "library.renamePlaceholder": "Enter a new title",
  "library.renameEmpty": "The new title cannot be empty.",
  "library.delete": "Delete",
  "library.deleting": "Deleting",
  "library.deleteConfirm": "Delete \"{title}\" from this Kindle?",
};

const koKR: MessageCatalog = {
  "app.name": "Kindle 전송",
  "app.topbarEyebrow": "Kindle 전송",
  "view.devices": "기기",
  "view.upload": "업로드",
  "view.library": "서재",
  "view.history": "기록",
  "view.devicesTitle": "기기 개요",
  "view.uploadTitle": "업로드 흐름",
  "view.libraryTitle": "Kindle 서재",
  "view.historyTitle": "전송 기록",
  "sidebar.devicesNote": "온라인 Kindle 기기",
  "sidebar.uploadNote": "변환 후 전송",
  "sidebar.libraryNote": "보기 및 삭제",
  "sidebar.historyNote": "전송 아카이브",
  "devices.summary.online": "온라인 기기",
  "devices.summary.onlineDetail": "현재 연결됨",
  "devices.summary.usbDetail": "직접 마운트",
  "devices.summary.lanDetail": "무선 경로",
  "devices.emptyTitle": "연결 가능한 Kindle을 아직 찾지 못했습니다",
  "devices.emptyBody": "Kindle이 USB로 연결되어 있고 운영체제가 저장소를 마운트했는지 확인하세요.",
  "devices.lanHintTitle": "USB 안내",
  "devices.lanHintBody": "현재 버전은 USB 직접 전송만 지원합니다. Kindle이 마운트되어 있고 documents 폴더가 보이는지 확인하세요.",
  "devices.uploadUnavailable": "현재 버전은 USB 직접 전송만 지원합니다.",
  "device.status.ready": "준비됨",
  "device.status.syncing": "동기화 중",
  "device.status.idle": "대기 중",
  "device.metric.battery": "배터리",
  "device.metric.storage": "저장공간",
  "device.metric.mount": "마운트 경로",
  "device.metric.address": "주소",
  "device.metric.storageLoad": "저장공간 사용량",
  "common.unknown": "알 수 없음",
  "upload.currentTarget": "현재 대상 기기",
  "upload.noDevice": "아직 Kindle 기기가 감지되지 않았습니다.",
  "upload.deviceUnavailable": "현재 버전은 USB 직접 전송만 지원합니다.",
  "upload.batchState": "배치 상태",
  "upload.transferRunning": "전송 중",
  "upload.startTransfer": "전송 시작",
  "upload.pausePipeline": "흐름 일시정지",
  "upload.overallProgress": "전체 진행률",
  "upload.targetFormatBias": "대상 형식",
  "upload.queueWeight": "대기열 용량",
  "upload.activeQueue": "현재 대기열",
  "upload.queueClear": "대기열이 비어 있음",
  "upload.live": "실시간",
  "upload.dropToStart": "전자책을 끌어다 놓으면 새 전송을 시작합니다.",
  "upload.finishedCount": "{done}/{total} 완료",
  "upload.processingCount": "{count}개 처리 중",
  "upload.localImport": "로컬 가져오기",
  "upload.progress.conversion": "변환",
  "upload.progress.upload": "업로드",
  "upload.writtenTo": "기록 위치",
  "upload.stage.done": "완료",
  "upload.stage.failed": "실패",
  "upload.stage.queued": "대기 중",
  "upload.stage.locked": "잠김",
  "upload.stage.running": "진행 중",
  "upload.stage.verifying": "검증 중",
  "upload.stage.waiting": "대기 중",
  "dropzone.tauriTitle": "전자책을 데스크톱 창으로 끌어오세요",
  "dropzone.browserTitle": "EPUB, MOBI, AZW3 또는 PDF를 끌어오세요",
  "dropzone.browseButton": "파일 선택",
  "dropzone.disabledDescription": "먼저 Kindle 기기를 선택해야 대기열의 전송 대상이 정해집니다.",
  "dropzone.tauriDescription": "데스크톱 창 드래그는 절대 경로를 Rust 백엔드로 전달해 바로 대기열에 넣고 업로드합니다.",
  "dropzone.browserDescription": "대상 Kindle을 확인하고 목차를 정리한 뒤 적합한 형식으로 변환하여 USB로 전송합니다.",
  "dropzone.feature.bulk": "일괄 대기열",
  "dropzone.feature.toc": "목차 복구",
  "dropzone.feature.format": "스마트 형식 매칭",
  "dialog.uploadTitle": "Kindle로 보낼 전자책 선택",
  "dialog.ebookFilter": "전자책",
  "history.eyebrow": "전송 기록",
  "history.title": "모든 전송 결과를 검색합니다.",
  "history.description": "최근 Kindle 세션의 형식 결정과 전송 결과를 확인합니다.",
  "history.metric.total": "전체",
  "history.metric.totalDetail": "기록된 작업",
  "history.metric.success": "성공",
  "history.metric.successDetail": "정상 완료",
  "history.metric.review": "확인 필요",
  "history.metric.reviewDetail": "부분 성공 또는 실패",
  "history.searchPlaceholder": "책 제목, 기기 또는 형식 검색",
  "history.filter.allStatus": "모든 상태",
  "history.filter.success": "성공",
  "history.filter.partial": "부분 성공",
  "history.filter.failed": "실패",
  "history.filter.allRoutes": "모든 경로",
  "history.table.book": "책",
  "history.table.target": "대상",
  "history.table.output": "출력",
  "history.table.finished": "완료 시간",
  "history.table.status": "상태",
  "history.status.success": "성공",
  "history.status.partial": "부분 성공",
  "history.status.failed": "실패",
  "history.empty": "아직 전송 기록이 없습니다.",
  "library.currentDevice": "현재 기기",
  "library.noDevice": "Kindle 미감지",
  "library.refresh": "서재 새로고침",
  "library.refreshing": "새로고침 중",
  "library.bookList": "기기 내 책",
  "library.booksUnit": "권",
  "library.loading": "Kindle 서재를 읽는 중입니다.",
  "library.empty": "이 Kindle의 documents 폴더에서 인식 가능한 책을 찾지 못했습니다.",
  "library.rename": "이름 변경",
  "library.renaming": "저장 중",
  "library.renameSave": "저장",
  "library.renameCancel": "취소",
  "library.renamePlaceholder": "새 책 제목 입력",
  "library.renameEmpty": "새 책 제목은 비워둘 수 없습니다.",
  "library.delete": "삭제",
  "library.deleting": "삭제 중",
  "library.deleteConfirm": "Kindle에서 「{title}」을 삭제할까요?",
};

const jaJP: MessageCatalog = {
  "app.name": "Kindle 転送",
  "app.topbarEyebrow": "Kindle 転送",
  "view.devices": "デバイス",
  "view.upload": "アップロード",
  "view.library": "ライブラリ",
  "view.history": "履歴",
  "view.devicesTitle": "デバイス概要",
  "view.uploadTitle": "アップロード手順",
  "view.libraryTitle": "Kindle ライブラリ",
  "view.historyTitle": "転送履歴",
  "sidebar.devicesNote": "オンライン Kindle",
  "sidebar.uploadNote": "変換して送信",
  "sidebar.libraryNote": "表示と削除",
  "sidebar.historyNote": "転送アーカイブ",
  "devices.summary.online": "オンライン",
  "devices.summary.onlineDetail": "現在利用可能",
  "devices.summary.usbDetail": "直接マウント",
  "devices.summary.lanDetail": "無線ルート",
  "devices.emptyTitle": "接続可能な Kindle がまだ見つかりません",
  "devices.emptyBody": "Kindle が USB で接続され、OS がストレージをマウントしていることを確認してください。",
  "devices.lanHintTitle": "USB メモ",
  "devices.lanHintBody": "現在のバージョンは USB 直接転送のみ対応しています。Kindle がマウントされ、documents フォルダが見えることを確認してください。",
  "devices.uploadUnavailable": "現在のバージョンは USB 直接転送のみ対応しています。",
  "device.status.ready": "準備完了",
  "device.status.syncing": "同期中",
  "device.status.idle": "待機中",
  "device.metric.battery": "バッテリー",
  "device.metric.storage": "ストレージ",
  "device.metric.mount": "マウント先",
  "device.metric.address": "アドレス",
  "device.metric.storageLoad": "ストレージ使用量",
  "common.unknown": "不明",
  "upload.currentTarget": "現在の送信先",
  "upload.noDevice": "Kindle デバイスがまだ検出されていません。",
  "upload.deviceUnavailable": "現在のバージョンは USB 直接転送のみ対応しています。",
  "upload.batchState": "バッチ状態",
  "upload.transferRunning": "転送中",
  "upload.startTransfer": "転送開始",
  "upload.pausePipeline": "手順を一時停止",
  "upload.overallProgress": "全体の進捗",
  "upload.targetFormatBias": "出力形式",
  "upload.queueWeight": "キュー容量",
  "upload.activeQueue": "現在のキュー",
  "upload.queueClear": "キューは空です",
  "upload.live": "ライブ",
  "upload.dropToStart": "電子書籍をドロップすると新しい転送を開始します。",
  "upload.finishedCount": "{done}/{total} 完了",
  "upload.processingCount": "{count} 件処理中",
  "upload.localImport": "ローカル取り込み",
  "upload.progress.conversion": "変換",
  "upload.progress.upload": "アップロード",
  "upload.writtenTo": "書き込み先",
  "upload.stage.done": "完了",
  "upload.stage.failed": "失敗",
  "upload.stage.queued": "待機中",
  "upload.stage.locked": "ロック中",
  "upload.stage.running": "実行中",
  "upload.stage.verifying": "検証中",
  "upload.stage.waiting": "待機中",
  "dropzone.tauriTitle": "電子書籍をデスクトップ画面へドロップ",
  "dropzone.browserTitle": "EPUB、MOBI、AZW3、PDF をドロップ",
  "dropzone.browseButton": "ファイルを選択",
  "dropzone.disabledDescription": "先に Kindle デバイスを選択すると、キューの送信先を正しく設定できます。",
  "dropzone.tauriDescription": "デスクトップへのドロップは絶対パスを Rust バックエンドへ渡し、そのままキュー登録とアップロードを行います。",
  "dropzone.browserDescription": "対象 Kindle を確認し、目次を整え、適切な形式へ変換して USB で送信します。",
  "dropzone.feature.bulk": "一括キュー",
  "dropzone.feature.toc": "目次修復",
  "dropzone.feature.format": "形式の自動判定",
  "dialog.uploadTitle": "Kindle に送る電子書籍を選択",
  "dialog.ebookFilter": "電子書籍",
  "history.eyebrow": "転送記録",
  "history.title": "すべての転送結果を検索します。",
  "history.description": "最近の Kindle セッションにおける形式選択と転送結果を確認できます。",
  "history.metric.total": "合計",
  "history.metric.totalDetail": "記録されたジョブ",
  "history.metric.success": "成功",
  "history.metric.successDetail": "正常完了",
  "history.metric.review": "要確認",
  "history.metric.reviewDetail": "一部成功または失敗",
  "history.searchPlaceholder": "書名、デバイス、形式を検索",
  "history.filter.allStatus": "すべての状態",
  "history.filter.success": "成功",
  "history.filter.partial": "一部成功",
  "history.filter.failed": "失敗",
  "history.filter.allRoutes": "すべての経路",
  "history.table.book": "書籍",
  "history.table.target": "送信先",
  "history.table.output": "出力",
  "history.table.finished": "完了時刻",
  "history.table.status": "状態",
  "history.status.success": "成功",
  "history.status.partial": "一部成功",
  "history.status.failed": "失敗",
  "history.empty": "転送履歴はまだありません。",
  "library.currentDevice": "現在のデバイス",
  "library.noDevice": "Kindle 未検出",
  "library.refresh": "ライブラリ更新",
  "library.refreshing": "更新中",
  "library.bookList": "デバイス内の書籍",
  "library.booksUnit": "冊",
  "library.loading": "Kindle ライブラリを読み込んでいます。",
  "library.empty": "この Kindle の documents フォルダに認識できる書籍はありません。",
  "library.rename": "名前変更",
  "library.renaming": "保存中",
  "library.renameSave": "保存",
  "library.renameCancel": "キャンセル",
  "library.renamePlaceholder": "新しい書名を入力",
  "library.renameEmpty": "新しい書名は空にできません。",
  "library.delete": "削除",
  "library.deleting": "削除中",
  "library.deleteConfirm": "Kindle から「{title}」を削除しますか？",
};

const messages: Record<Locale, MessageCatalog> = {
  "zh-CN": zhCN,
  "en-US": enUS,
  "ko-KR": koKR,
  "ja-JP": jaJP,
};

function isLocale(value: string | null): value is Locale {
  return LOCALE_OPTIONS.some((option) => option.value === value);
}

function syncDocumentLanguage(locale: Locale) {
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale;
  }
}

function resolveInitialLocale(): Locale {
  if (typeof window === "undefined") {
    return DEFAULT_LOCALE;
  }

  const storedLocale = window.localStorage.getItem(LOCALE_STORAGE_KEY);
  if (isLocale(storedLocale)) {
    return storedLocale;
  }

  window.localStorage.setItem(LOCALE_STORAGE_KEY, DEFAULT_LOCALE);
  return DEFAULT_LOCALE;
}

let currentLocale: Locale = resolveInitialLocale();
syncDocumentLanguage(currentLocale);

export function getCurrentLocale() {
  return currentLocale;
}

export function setCurrentLocale(locale: Locale) {
  currentLocale = locale;
  if (typeof window !== "undefined") {
    window.localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  }
  syncDocumentLanguage(locale);
}

export function t(key: MessageKey): string {
  return messages[currentLocale][key] ?? messages[DEFAULT_LOCALE][key];
}

export function formatMessage(key: MessageKey, values: Record<string, string | number>): string {
  return Object.entries(values).reduce(
    (message, [name, value]) => message.split(`{${name}}`).join(String(value)),
    t(key),
  );
}

export function formatDeviceStatus(status: DeviceStatus): string {
  return t(`device.status.${status}` as MessageKey);
}

export function formatHistoryStatus(status: HistoryStatus): string {
  return t(`history.status.${status}` as MessageKey);
}

export function formatQueueStageCaption(
  stage: QueueStage,
  lane: "convert" | "upload",
): string {
  if (stage === "done") {
    return t("upload.stage.done");
  }

  if (stage === "failed") {
    return t("upload.stage.failed");
  }

  if (lane === "convert") {
    if (stage === "queued") {
      return t("upload.stage.queued");
    }

    if (stage === "uploading" || stage === "verifying") {
      return t("upload.stage.locked");
    }

    return t("upload.stage.running");
  }

  if (stage === "uploading") {
    return t("upload.stage.running");
  }

  if (stage === "verifying") {
    return t("upload.stage.verifying");
  }

  return t("upload.stage.waiting");
}
