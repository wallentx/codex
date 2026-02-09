use hickory_resolver::Resolver;
use hickory_resolver::TokioResolver;
use hickory_resolver::config::NameServerConfig;
use hickory_resolver::proto::xfer::Protocol;
use hickory_resolver::config::ResolverConfig;
use hickory_resolver::config::ResolverOpts;
use hickory_resolver::name_server::TokioConnectionProvider;
use reqwest::dns::Addrs;
use reqwest::dns::Name;
use reqwest::dns::Resolve;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Clone)]
pub struct TermuxResolver {
    resolver: Arc<TokioResolver>,
}

impl TermuxResolver {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Try to load standard system configuration.
        // This handles standard Linux environments where /etc/resolv.conf is present and valid.
        let (config, _opts) =
            if let Ok((config, opts)) = hickory_resolver::system_conf::read_system_conf() {
                (config, opts)
            } else {
                // 2. Fallback for environments where /etc/resolv.conf is missing or inaccessible.
                // On Android/Termux, the resolver configuration is typically located at $PREFIX/etc/resolv.conf.
                let prefix = std::env::var("PREFIX").unwrap_or_default();
                let path = PathBuf::from(prefix).join("etc/resolv.conf");

                let mut config = ResolverConfig::new();
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Simple parser for 'nameserver' entries in resolv.conf
                    for line in content.lines() {
                        let line = line.trim();
                        if line.starts_with("nameserver ") {
                            let ip_str = line.trim_start_matches("nameserver ").trim();
                            if let Ok(ip) = ip_str.parse() {
                                config.add_name_server(NameServerConfig::new(
                                    SocketAddr::new(ip, 53),
                                    Protocol::Udp,
                                ));
                            }
                        }
                    }
                }

                // If we couldn't find any nameservers in the alternative path,
                // we'll default to an empty config which will cause the builder
                // to use its own internal defaults (usually system defaults).
                (config, ResolverOpts::default())
            };

        let resolver =
            Resolver::builder_with_config(config, TokioConnectionProvider::default()).build();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_termux_resolver_new() {
        let resolver = TermuxResolver::new();
        assert!(resolver.is_ok());
    }

    #[test]
    fn test_parse_resolv_conf_contents() {
        let content = "nameserver 1.1.1.1\nnameserver 8.8.8.8\n# comment\n  nameserver 9.9.9.9  ";
        let mut config = ResolverConfig::new();
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("nameserver ") {
                let ip_str = line.trim_start_matches("nameserver ").trim();
                if let Ok(ip) = ip_str.parse() {
                    config.add_name_server(NameServerConfig::new(
                        SocketAddr::new(ip, 53),
                        Protocol::Udp,
                    ));
                }
            }
        }
        assert_eq!(config.name_servers().len(), 3);
        assert_eq!(config.name_servers()[0].socket_addr.ip().to_string(), "1.1.1.1");
        assert_eq!(config.name_servers()[1].socket_addr.ip().to_string(), "8.8.8.8");
        assert_eq!(config.name_servers()[2].socket_addr.ip().to_string(), "9.9.9.9");
    }
}
