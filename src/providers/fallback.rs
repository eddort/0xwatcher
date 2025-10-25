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
    pub rpc_urls: Vec<Url>,
    pub active_transport_count: NonZeroUsize,
}

impl FallbackConfig {
    pub fn new(rpc_urls: Vec<Url>, active_transport_count: NonZeroUsize) -> Self {
        Self {
            rpc_urls,
            active_transport_count,
        }
    }
}

/// Creates a provider with fallback support
pub fn create_fallback_provider(
    config: FallbackConfig,
) -> Result<impl alloy::providers::Provider> {
    let fallback_layer = FallbackLayer::default()
        .with_active_transport_count(config.active_transport_count);

    let transports: Vec<Http<_>> = config
        .rpc_urls
        .into_iter()
        .map(Http::new)
        .collect();

    let transport = ServiceBuilder::new()
        .layer(fallback_layer)
        .service(transports);

    let client = RpcClient::builder().transport(transport, false);
    let provider = ProviderBuilder::new().connect_client(client);

    Ok(provider)
}
