# SFTP theo Terminal CWD — Thiết kế (chia phần)

Tính năng: **nút trên SFTP Browser để chuyển tới thư mục hiện tại (`cwd`) của SSH
session**. User `cd` trong terminal → bấm nút → SFTP nhảy theo.

Tài liệu chia nhỏ để dễ đọc/review; bản gộp đầy đủ ở
[`../sftp-follow-terminal-cwd.md`](../sftp-follow-terminal-cwd.md).

| Phần | File | Nội dung |
|------|------|----------|
| 1 | [`01-overview.md`](01-overview.md) | Tổng quan, mục tiêu, yêu cầu, giả định OSC 7, manual vs auto-follow |
| 2 | [`02-current-state.md`](02-current-state.md) | Hiện trạng codebase: `cwd()`, `SftpPanel.load_dir`, `active_sftp`, gap analysis |
| 3 | [`03-high-level-design.md`](03-high-level-design.md) | Kiến trúc, `CwdSource`, luồng dữ liệu, trạng thái nút |
| 4 | [`04-low-level-design.md`](04-low-level-design.md) | Struct, chữ ký hàm, code mẫu từng crate, bảng thay đổi file |
| 5 | [`05-edge-cases-roadmap.md`](05-edge-cases-roadmap.md) | Edge cases, rủi ro, kiểm thử, roadmap, DoD |

> Sau khi review từng phần, chạy lại bước gộp để cập nhật bản `../sftp-follow-terminal-cwd.md`.
