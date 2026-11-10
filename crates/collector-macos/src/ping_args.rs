//! Destination token from a `ping(8)` argument vector.
//!
//! macOS `ping` keeps an unconnected ICMP socket. The remote host is only
//! present in the process arguments.

/// Short options that consume a value (`-c 3` or `-c3`).
const OPTIONS_WITH_VALUE: &[char] = &[
    'b', 'c', 'G', 'g', 'h', 'I', 'i', 'K', 'k', 'l', 'M', 'm', 'P', 'p', 'S', 's', 'T', 't', 'W',
    'z',
];

/// Returns the host positional argument from `ping` / `ping6` argv.
///
/// `args[0]` is the program name. Flag parsing follows the macOS `ping` synopsis.
pub fn ping_destination(args: &[String]) -> Option<&str> {
    if args.len() < 2 {
        return None;
    }
    let mut index = 1usize;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--" {
            return args
                .get(index + 1)
                .map(String::as_str)
                .filter(|host| !host.is_empty());
        }
        if arg.starts_with("--") {
            index += 1;
            continue;
        }
        if let Some(flags) = arg.strip_prefix('-') {
            if flags.is_empty() {
                index += 1;
                continue;
            }
            let chars: Vec<char> = flags.chars().collect();
            let mut cursor = 0;
            while cursor < chars.len() {
                let flag = chars[cursor];
                if OPTIONS_WITH_VALUE.contains(&flag) {
                    let inline: String = chars[cursor + 1..].iter().collect();
                    if inline.is_empty() {
                        // Value is the next argv element (`-c 3`).
                        index += 1;
                    }
                    break;
                }
                cursor += 1;
            }
            index += 1;
            continue;
        }
        if arg.is_empty() {
            index += 1;
            continue;
        }
        return Some(arg);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    #[test]
    fn host_is_the_first_positional() {
        assert_eq!(
            ping_destination(&args(&["ping", "chatgpt.com"])),
            Some("chatgpt.com")
        );
    }

    #[test]
    fn skips_count_and_numeric_attached_value() {
        assert_eq!(
            ping_destination(&args(&["ping", "-c", "3", "172.64.155.209"])),
            Some("172.64.155.209")
        );
        assert_eq!(
            ping_destination(&args(&["ping", "-c3", "172.64.155.209"])),
            Some("172.64.155.209")
        );
    }

    #[test]
    fn skips_clustered_flags_and_source_address() {
        assert_eq!(
            ping_destination(&args(&["ping", "-nq", "-S", "10.0.0.1", "chatgpt.com"])),
            Some("chatgpt.com")
        );
        assert_eq!(
            ping_destination(&args(&["ping", "-nc", "2", "example.com"])),
            Some("example.com")
        );
    }

    #[test]
    fn skips_long_options_and_honors_end_of_flags() {
        assert_eq!(
            ping_destination(&args(&["ping", "--apple-connect", "chatgpt.com"])),
            Some("chatgpt.com")
        );
        assert_eq!(
            ping_destination(&args(&["ping", "--", "-weird.example"])),
            Some("-weird.example")
        );
    }

    #[test]
    fn ignores_trailing_packet_size() {
        assert_eq!(
            ping_destination(&args(&["ping", "chatgpt.com", "56"])),
            Some("chatgpt.com")
        );
    }

    #[test]
    fn missing_host_is_none() {
        assert_eq!(ping_destination(&args(&["ping"])), None);
        assert_eq!(ping_destination(&args(&["ping", "-c", "3"])), None);
        assert_eq!(ping_destination(&[]), None);
    }
}
