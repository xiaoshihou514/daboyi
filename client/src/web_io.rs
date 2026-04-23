#[cfg(target_arch = "wasm32")]
use gloo_net::http::Request;

#[cfg(target_arch = "wasm32")]
pub async fn fetch_bytes(path: &'static str) -> Result<Vec<u8>, String> {
    let response = Request::get(path)
        .send()
        .await
        .map_err(|error| format!("请求 {path} 失败：{error}"))?;
    if !response.ok() {
        return Err(format!("请求 {path} 失败：HTTP {}", response.status()));
    }
    response
        .binary()
        .await
        .map_err(|error| format!("读取 {path} 二进制内容失败：{error}"))
}

#[cfg(target_arch = "wasm32")]
pub async fn fetch_text(path: &'static str) -> Result<String, String> {
    let response = Request::get(path)
        .send()
        .await
        .map_err(|error| format!("请求 {path} 失败：{error}"))?;
    if !response.ok() {
        return Err(format!("请求 {path} 失败：HTTP {}", response.status()));
    }
    response
        .text()
        .await
        .map_err(|error| format!("读取 {path} 文本内容失败：{error}"))
}
