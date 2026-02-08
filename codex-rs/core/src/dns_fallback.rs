use hickory_resolver::Resolver;
use hickory_resolver::TokioResolver;
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
        // 1. Standard system configuration
        let (config, _opts) =
            if let Ok((config, opts)) = hickory_resolver::system_conf::read_system_conf() {
                (config, opts)
            } else {
                // 2. Termux fallback
                let prefix = std::env::var("PREFIX").unwrap_or_default();
                let path = PathBuf::from(prefix).join("etc/resolv.conf");
                if path.exists() {
                    (ResolverConfig::google(), ResolverOpts::default())
                } else {
                    (ResolverConfig::google(), ResolverOpts::default())
                }
            };

        // Construct using the builder pattern which seems to be the way in 0.25
        // We might be losing `opts` here if we don't set them, but defaults are usually fine.
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
