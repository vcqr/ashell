//! 小型通用工具：UTF-8 分块安全拼接、ANSI 转义序列剥离。
//! 主要供终端输出的 AI 收集副本使用，不影响终端原始渲染。

/// 按块累积字节并按 UTF-8 合法边界切分：完整字符立即转字符串返回，尾部
/// 不完整的多字节序列扣留到下一块补齐，避免跨块字符被 lossy 转成乱码。
/// 块内出现非法字节（非「截断于字符中间」）时，连同其后内容一并 lossy 处理。
pub fn utf8_safe(tail: &mut Vec<u8>, data: &[u8]) -> String {
    tail.extend_from_slice(data);
    let valid = match std::str::from_utf8(tail) {
        Ok(_) => tail.len(),
        Err(e) => e.valid_up_to(),
    };
    let mut out = String::from_utf8_lossy(&tail[..valid]).into_owned();
    let rest = tail.split_off(valid);
    match std::str::from_utf8(&rest) {
        // 截断于字符中间（待续字节最多 3 个）：扣留待下一块补齐
        Err(e) if e.error_len().is_none() && rest.len() <= 3 => {
            *tail = rest;
        }
        _ => {
            if !rest.is_empty() {
                out.push_str(&String::from_utf8_lossy(&rest));
            }
            tail.clear();
        }
    }
    out
}

/// 冲刷 [`utf8_safe`] 扣留的尾缓冲：等不齐的残字节按 lossy 处理。
/// 会话输出结束（中继循环退出）时调用，避免订阅者永远收不到最后一截。
pub fn utf8_flush(tail: &mut Vec<u8>) -> String {
    if tail.is_empty() {
        return String::new();
    }
    let out = String::from_utf8_lossy(tail).into_owned();
    tail.clear();
    out
}

/// 剥离 ANSI/ECMA-48 转义序列：CSI（`ESC [ … 终止字节`）、字符串序列
/// （OSC/DCS/SOS/PM/APC，BEL 或 ST 结尾）、nF 中间字节序列（`ESC ( B` 等）
/// 与双字节转义（`ESC =` 等），其余字符（含 `\r\n`、多字节文本）原样保留。
/// 输入为合法 UTF-8 时输出亦然：所有被剥离的字节均为 ASCII。
pub fn strip_ansi(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != 0x1b {
            out.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i >= b.len() {
            break; // 悬空 ESC，丢弃
        }
        match b[i] {
            b'[' => {
                // CSI：参数/中间字节 0x20-0x3F，终止字节 0x40-0x7E
                i += 1;
                while i < b.len() && !(0x40..=0x7e).contains(&b[i]) {
                    i += 1;
                }
                if i < b.len() {
                    i += 1;
                }
            }
            b']' | b'P' | b'X' | b'^' | b'_' => {
                // 字符串序列：BEL 或 ST（ESC \）结束
                i += 1;
                while i < b.len() {
                    if b[i] == 0x07 {
                        i += 1;
                        break;
                    }
                    if b[i] == 0x1b && b.get(i + 1) == Some(&b'\\') {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            // nF 序列：中间字节 0x20-0x2F + 一个终止字节（ESC # 8、ESC ( B …）
            x if (0x20..=0x2f).contains(&x) => {
                while i < b.len() && (0x20..=0x2f).contains(&b[i]) {
                    i += 1;
                }
                if i < b.len() {
                    i += 1;
                }
            }
            // 双字节转义（ESC =、ESC >、ESC M …）：ESC 后的字节即终止
            _ => {
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_safe_splits_multibyte_across_chunks() {
        let b = "中文输出".as_bytes();
        let mut tail = Vec::new();
        let first = utf8_safe(&mut tail, &b[..5]); // 「中」(3B) + 「文」前 2 字节
        assert_eq!(first, "中");
        let second = utf8_safe(&mut tail, &b[5..]);
        assert_eq!(format!("{first}{second}"), "中文输出");
        assert!(tail.is_empty());
    }

    #[test]
    fn utf8_safe_ascii_passthrough() {
        let mut tail = Vec::new();
        assert_eq!(utf8_safe(&mut tail, b"hello "), "hello ");
        assert_eq!(utf8_safe(&mut tail, b"world"), "world");
        assert!(tail.is_empty());
    }

    #[test]
    fn utf8_safe_invalid_byte_goes_lossy() {
        let mut tail = Vec::new();
        let out = utf8_safe(&mut tail, b"a\xffb");
        assert_eq!(out, "a\u{fffd}b");
        assert!(tail.is_empty());
    }

    #[test]
    fn utf8_flush_drains_pending_tail() {
        let b = "输".as_bytes();
        let mut tail = Vec::new();
        let _ = utf8_safe(&mut tail, &b[..2]);
        // 残字节等不齐，冲刷只保证「交付 + 清空」，不承诺还原完整字符
        assert!(!tail.is_empty());
        let flushed = utf8_flush(&mut tail);
        assert!(!flushed.is_empty());
        assert!(tail.is_empty());
    }

    #[test]
    fn strip_ansi_removes_csi_and_bel_osc() {
        let s = "\x1b[1;32muser\x1b[0m:~$ \x1b]0;title\x07ls\r\n";
        assert_eq!(strip_ansi(s), "user:~$ ls\r\n");
    }

    #[test]
    fn strip_ansi_handles_st_terminated_string_seq() {
        let s = "\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\ok";
        assert_eq!(strip_ansi(s), "linkok");
    }

    #[test]
    fn strip_ansi_handles_two_byte_and_nf_escapes() {
        assert_eq!(strip_ansi("\x1b=abc"), "abc"); // ESC = 小键盘模式
        assert_eq!(strip_ansi("\x1b(Babc"), "abc"); // ESC ( B 字符集
        assert_eq!(strip_ansi("\x1b#8abc"), "abc"); // ESC # 8 DECALN
    }

    #[test]
    fn strip_ansi_keeps_plain_text() {
        assert_eq!(strip_ansi("total 120\n中文 ok\r\n"), "total 120\n中文 ok\r\n");
    }

    #[test]
    fn strip_ansi_dangling_and_unterminated_dropped() {
        assert_eq!(strip_ansi("abc\x1b"), "abc");
        assert_eq!(strip_ansi("abc\x1b]9;9;/ho"), "abc"); // OSC 未终结
    }
}
