use hickory_resolver::Resolver;
use hickory_resolver::TokioResolver;
use hickory_resolver::config::NameServerConfig;
use hickory_resolver::config::ResolverConfig;
use hickory_resolver::config::ResolverOpts;
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::proto::xfer::Protocol;
use reqwest::dns::Addrs;
use reqwest::dns::Name;
use reqwest::dns::Resolve;
use std::future::Future;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Clone)]
pub struct TermuxResolver {
    resolver: Arc<TokioResolver>,
}

pub fn should_install_termux_resolver() -> bool {
    should_install_termux_resolver_with(
        cfg!(target_os = "android"),
        std::env::var_os("TERMUX_VERSION"),
        std::env::var_os("PREFIX"),
    )
}

impl TermuxResolver {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (config, options) = resolver_config_and_options();
        let mut builder = Resolver::builder_with_config(config, TokioConnectionProvider::default());
        *builder.options_mut() = options;
        let resolver = builder.build();

        Ok(Self {
            resolver: Arc::new(resolver),
        })
    }
}

impl Resolve for TermuxResolver {
    fn resolve(
        &self,
        name: Name,
    ) -> Pin<Box<dyn Future<Output = Result<Addrs, Box<dyn std::error::Error + Send + Sync>>> + Send>>
    {
        let resolver = self.resolver.clone();
        Box::pin(async move {
            let lookup = resolver.lookup_ip(name.as_str()).await?;
            let addrs: Addrs = Box::new(lookup.into_iter().map(|ip| SocketAddr::new(ip, 0)));
            Ok(addrs)
        })
    }
}

fn resolver_config_and_options() -> (ResolverConfig, ResolverOpts) {
    if let Ok((config, options)) = hickory_resolver::system_conf::read_system_conf() {
        return (config, options);
    }

    let mut config = ResolverConfig::new();
    if let Ok(content) = read_prefix_resolv_conf() {
        add_nameservers_from_resolv_conf(&content, &mut config);
    }

    if config.name_servers().is_empty() {
        return (ResolverConfig::google(), ResolverOpts::default());
    }

    (config, ResolverOpts::default())
}

fn read_prefix_resolv_conf() -> Result<String, std::io::Error> {
    let prefix = std::env::var("PREFIX").unwrap_or_default();
    let path = PathBuf::from(prefix).join("etc/resolv.conf");
    std::fs::read_to_string(path)
}

fn add_nameservers_from_resolv_conf(content: &str, config: &mut ResolverConfig) {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("nameserver ")
            && let Some(ip) = line
                .trim_start_matches("nameserver ")
                .trim()
                .parse::<IpAddr>()
                .ok()
        {
            add_nameserver_for_ip(config, ip);
        }
    }
}

fn add_nameserver_for_ip(config: &mut ResolverConfig, ip: IpAddr) {
    config.add_name_server(NameServerConfig::new(
        SocketAddr::new(ip, 53),
        Protocol::Udp,
    ));
    config.add_name_server(NameServerConfig::new(
        SocketAddr::new(ip, 53),
        Protocol::Tcp,
    ));
}

fn should_install_termux_resolver_with(
    is_android: bool,
    termux_version: Option<std::ffi::OsString>,
    prefix: Option<std::ffi::OsString>,
) -> bool {
    is_android
        || termux_version.is_some()
        || prefix
            .and_then(|value| value.into_string().ok())
            .is_some_and(|prefix| prefix.contains("/com.termux/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_termux_resolver_new() {
        let resolver = TermuxResolver::new();
        assert!(resolver.is_ok());
    }

    #[test]
    fn test_parse_resolv_conf_contents() {
        let content = "nameserver 1.1.1.1\nnameserver 8.8.8.8\n# comment\n  nameserver 9.9.9.9  ";
        let mut config = ResolverConfig::new();
        add_nameservers_from_resolv_conf(content, &mut config);
        assert_eq!(config.name_servers().len(), 6);
        assert_eq!(
            config.name_servers()[0].socket_addr.ip().to_string(),
            "1.1.1.1"
        );
        assert_eq!(
            config.name_servers()[1].socket_addr.ip().to_string(),
            "1.1.1.1"
        );
        assert_eq!(
            config.name_servers()[2].socket_addr.ip().to_string(),
            "8.8.8.8"
        );
        assert_eq!(
            config.name_servers()[3].socket_addr.ip().to_string(),
            "8.8.8.8"
        );
        assert_eq!(
            config.name_servers()[4].socket_addr.ip().to_string(),
            "9.9.9.9"
        );
        assert_eq!(
            config.name_servers()[5].socket_addr.ip().to_string(),
            "9.9.9.9"
        );
    }

    #[test]
    fn test_add_nameserver_for_ip_adds_udp_and_tcp() {
        let mut config = ResolverConfig::new();
        add_nameserver_for_ip(&mut config, "1.1.1.1".parse().expect("valid ip"));
        let protocols = config
            .name_servers()
            .iter()
            .map(|server| server.protocol)
            .collect::<Vec<_>>();
        assert_eq!(protocols, vec![Protocol::Udp, Protocol::Tcp]);
    }

    #[test]
    fn should_install_termux_resolver_detects_signals() {
        assert_eq!(
            should_install_termux_resolver_with(false, None, None),
            false
        );
        assert_eq!(
            should_install_termux_resolver_with(
                false,
                Some(std::ffi::OsString::from("0.118.0")),
                None,
            ),
            true
        );
        assert_eq!(
            should_install_termux_resolver_with(
                false,
                None,
                Some(std::ffi::OsString::from("/data/data/com.termux/files/usr")),
            ),
            true
        );
        assert_eq!(
            should_install_termux_resolver_with(
                false,
                None,
                Some(std::ffi::OsString::from("/usr"))
            ),
            false
        );
        assert_eq!(should_install_termux_resolver_with(true, None, None), true);
    }
}
