use alloy::{
    providers::ProviderBuilder,
    rpc::client::RpcClient,
    transports::{
        http::{reqwest::Url, Http},
        layers::FallbackLayer,
    },
};
use eyre::Result;
use std::num::NonZeroUsize;
use tower::ServiceBuilder;

/// Configuration for fallback provider
pub struct FallbackConfig {
    pub rpc_urls: Vec<String>,
    pub active_transport_count: usize,
}

impl FallbackConfig {
    pub fn new(rpc_urls: Vec<String>, active_transport_count: usize) -> Self {
        Self {
            rpc_urls,
            active_transport_count,
        }
    }

    pub fn with_active_count(mut self, count: usize) -> Self {
        self.active_transport_count = count;
        self
    }
}

/// Creates a provider with fallback support
pub fn create_fallback_provider(
    config: FallbackConfig,
) -> Result<impl alloy::providers::Provider> {
    let fallback_layer = FallbackLayer::default().with_active_transport_count(
        NonZeroUsize::new(config.active_transport_count).unwrap_or(NonZeroUsize::new(3).unwrap()),
    );

    let transports: Result<Vec<Http<_>>> = config
        .rpc_urls
        .iter()
        .map(|url| {
            Url::parse(url)
                .map(Http::new)
                .map_err(|e| eyre::eyre!("Invalid URL {}: {}", url, e))
        })
        .collect();

    let transports = transports?;

    let transport = ServiceBuilder::new()
        .layer(fallback_layer)
        .service(transports);

    let client = RpcClient::builder().transport(transport, false);
    let provider = ProviderBuilder::new().connect_client(client);

    Ok(provider)
}
