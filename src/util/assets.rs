//! Cache-busting for bundled assets.
//!
//! See `build.rs` for why this is needed: icon URLs never change, and nothing
//! in the response tells the WebView to revalidate, so an edited icon can go
//! on serving from cache for weeks. `ASSET_VERSION` is a hash of `public/`,
//! so the query changes exactly when an asset's bytes do.
//!
//! Safe on both paths: trunk's dev server ignores the query like any static
//! file server, and Tauri's asset protocol splits on `?` before resolving.

/// Tag an asset path with the current asset-set hash.
pub fn asset(path: &str) -> String {
    format!("{path}?v={}", env!("ASSET_VERSION"))
}
