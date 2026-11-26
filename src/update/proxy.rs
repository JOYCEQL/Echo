use std::process::Command;

struct Route {
    endpoint: String,
    exceptions: Vec<String>,
}

pub(super) fn configure(curl: &mut Command) {
    // Leave curl in charge when the user supplies a proxy in the environment.
    let has_override = [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "http_proxy",
    ]
    .iter()
    .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()));
    if has_override {
        return;
    }
    if let Some(route) = platform_route() {
        curl.arg("--proxy").arg(route.endpoint);
        if std::env::var_os("no_proxy").is_none() && std::env::var_os("NO_PROXY").is_none() {
            let exceptions = route
                .exceptions
                .iter()
                .filter_map(|entry| curl_exception(entry));
            let mut hosts = vec![
                String::from("localhost"),
                String::from("127.0.0.1"),
                String::from("::1"),
            ];
            hosts.extend(exceptions);
            curl.arg("--noproxy").arg(hosts.join(","));
        }
    }
}

fn curl_exception(entry: &str) -> Option<String> {
    let pattern = entry.trim();
    if pattern == "*" {
        return Some(pattern.to_owned());
    }
    let suffix = pattern.trim_start_matches("*.").trim_start_matches('.');
    if suffix.is_empty() || suffix.starts_with('<') || suffix.contains(',') {
        return None;
    }
    // System network settings accept abbreviated IPv4 networks and trailing wildcard octets.
    if suffix.contains('/') || suffix.contains('*') {
        let (network, prefix) = suffix
            .split_once('/')
            .map_or((suffix, None), |(network, bits)| (network, Some(bits)));
        if network.contains(':') {
            network.parse::<std::net::Ipv6Addr>().ok()?;
            return (prefix?.parse::<u8>().ok()? <= 128).then(|| suffix.to_owned());
        }
        let mut bytes = [0; 4];
        let mut fixed = 0;
        let mut wildcard = false;
        for (index, octet) in network.split('.').enumerate() {
            if index >= bytes.len() {
                return None;
            }
            if octet == "*" {
                wildcard = true;
            } else {
                if wildcard {
                    return None;
                }
                bytes[index] = octet.parse().ok()?;
                fixed += 1;
            }
        }
        let bits = match prefix {
            Some(bits) if !wildcard => bits.parse::<u8>().ok()?,
            None if wildcard && fixed > 0 => fixed * 8,
            _ => return None,
        };
        return (bits <= 32).then(|| format!("{}/{bits}", std::net::Ipv4Addr::from(bytes)));
    }
    Some(suffix.to_owned())
}

#[cfg(target_os = "macos")]
fn platform_route() -> Option<Route> {
    use core_foundation::{
        array::CFArray,
        base::{CFType, TCFType},
        dictionary::{CFDictionary, CFDictionaryRef},
        number::CFNumber,
        string::CFString,
    };
    #[link(name = "CFNetwork", kind = "framework")]
    unsafe extern "C" {
        fn CFNetworkCopySystemProxySettings() -> CFDictionaryRef;
    }

    // The Copy function transfers ownership. Its dictionary is a property list with string keys.
    let raw = unsafe { CFNetworkCopySystemProxySettings() };
    if raw.is_null() {
        return None;
    }
    let settings: CFDictionary<CFString, CFType> =
        unsafe { CFDictionary::wrap_under_create_rule(raw) };
    let number = |key: &str| {
        settings
            .find(&CFString::new(key))?
            .downcast::<CFNumber>()?
            .to_i32()
    };
    let text = |key: &str| {
        Some(
            settings
                .find(&CFString::new(key))?
                .downcast::<CFString>()?
                .to_string(),
        )
    };
    for (service, protocol) in [("HTTPS", "http"), ("HTTP", "http"), ("SOCKS", "socks5h")] {
        if number(&format!("{service}Enable")) != Some(1) {
            continue;
        }
        let Some(host) = text(&format!("{service}Proxy")).filter(|s| !s.is_empty()) else {
            continue;
        };
        let authority = match host.parse::<std::net::Ipv6Addr>() {
            Ok(address) => format!("[{address}]"),
            Err(_) => host,
        };
        let mut endpoint = format!("{protocol}://{authority}");
        if let Some(port) = number(&format!("{service}Port"))
            .and_then(|p| u16::try_from(p).ok())
            .filter(|p| *p > 0)
        {
            endpoint.push_str(&format!(":{port}"));
        }
        let mut exceptions = Vec::new();
        if let Some(list) = settings
            .find(&CFString::new("ExceptionsList"))
            .and_then(|v| v.downcast::<CFArray>())
        {
            for entry in list.iter() {
                // Property list arrays contain CF objects. Verify the element type before reading it.
                let value = unsafe { CFType::wrap_under_get_rule(*entry) };
                if let Some(host) = value.downcast::<CFString>() {
                    exceptions.push(host.to_string());
                }
            }
        }
        return Some(Route {
            endpoint,
            exceptions,
        });
    }
    None
}

#[cfg(windows)]
fn platform_route() -> Option<Route> {
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        Networking::WinHttp::{
            WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WinHttpGetIEProxyConfigForCurrentUser,
        },
    };
    // WinHTTP returns independent GlobalAlloc strings; release all three, including the unused PAC URL.
    let mut config: WINHTTP_CURRENT_USER_IE_PROXY_CONFIG = unsafe { std::mem::zeroed() };
    if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut config) } == 0 {
        return None;
    }
    fn take(ptr: *mut u16) -> String {
        if ptr.is_null() {
            return String::new();
        }
        unsafe {
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            GlobalFree(ptr.cast());
            text
        }
    }
    let servers = take(config.lpszProxy);
    let bypass = take(config.lpszProxyBypass);
    if !config.lpszAutoConfigUrl.is_null() {
        unsafe {
            GlobalFree(config.lpszAutoConfigUrl.cast());
        }
    }
    Some(Route {
        endpoint: windows_endpoint(&servers)?,
        exceptions: bypass.split([';', ' ']).map(str::to_owned).collect(),
    })
}

#[cfg(any(windows, test))]
fn windows_endpoint(servers: &str) -> Option<String> {
    let mut routes = Vec::new();
    for item in servers
        .split(|ch: char| ch == ';' || ch.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
    {
        let (scope, address) = item.split_once('=').unwrap_or(("", item));
        let (priority, scheme) = match scope.to_ascii_lowercase().as_str() {
            "https" => (0, "http"),
            "" => (1, "http"),
            "http" => (2, "http"),
            "socks" => (3, "socks4a"),
            _ => continue,
        };
        if !address.is_empty() {
            routes.push((priority, scheme, address));
        }
    }
    let (_, scheme, address) = routes.into_iter().min_by_key(|route| route.0)?;
    Some(if address.contains("://") {
        address.to_owned()
    } else {
        format!("{scheme}://{address}")
    })
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_route() -> Option<Route> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exceptions_cover_domain_and_network_rules() {
        for (rule, expected) in [
            (" *.corp.example ", Some("corp.example")),
            ("172.21.*.*", Some("172.21.0.0/16")),
            ("100.64/10", Some("100.64.0.0/10")),
            ("fd00::/8", Some("fd00::/8")),
            ("*", Some("*")),
            ("<local>", None),
            ("1.*.2", None),
            ("203.0/33", None),
        ] {
            assert_eq!(curl_exception(rule).as_deref(), expected, "{rule}");
        }
    }

    #[test]
    fn chooses_a_proxy_for_an_https_download() {
        for (config, expected) in [
            (
                "ftp=files.invalid; socks=s:1 HTTP=h:2; HTTPS=t:3",
                Some("http://t:3"),
            ),
            ("http=h:2;shared:80", Some("http://shared:80")),
            ("socks=[::1]:7777", Some("socks4a://[::1]:7777")),
            ("https=socks5h://edge:1081", Some("socks5h://edge:1081")),
            ("ftp=ftp-only:21 https=", None),
        ] {
            assert_eq!(windows_endpoint(config).as_deref(), expected, "{config}");
        }
    }
}
