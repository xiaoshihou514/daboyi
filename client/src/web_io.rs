#[cfg(target_arch = "wasm32")]
use gloo_net::http::Request;
#[cfg(target_arch = "wasm32")]
use shared::ColoringFile;
#[cfg(target_arch = "wasm32")]
use std::sync::{Mutex, OnceLock};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::{spawn_local, JsFuture};
#[cfg(target_arch = "wasm32")]
use web_sys::{Blob, HtmlAnchorElement, HtmlInputElement, Url};

#[cfg(target_arch = "wasm32")]
fn coloring_load_slot() -> &'static Mutex<Option<Result<ColoringFile, String>>> {
    static SLOT: OnceLock<Mutex<Option<Result<ColoringFile, String>>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[cfg(target_arch = "wasm32")]
struct ModalTextInput {
    input: HtmlInputElement,
    active_field: Option<String>,
}

#[cfg(target_arch = "wasm32")]
fn modal_text_input_slot() -> &'static Mutex<Option<ModalTextInput>> {
    static SLOT: OnceLock<Mutex<Option<ModalTextInput>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[cfg(target_arch = "wasm32")]
fn ensure_modal_text_input() -> Result<HtmlInputElement, String> {
    if let Some(existing) = modal_text_input_slot()
        .lock()
        .unwrap()
        .as_ref()
        .map(|state| state.input.clone())
    {
        return Ok(existing);
    }

    let window = web_sys::window().ok_or("浏览器窗口不可用".to_string())?;
    let document = window.document().ok_or("浏览器文档不可用".to_string())?;
    let body = document.body().ok_or("浏览器页面缺少 body".to_string())?;

    let input: HtmlInputElement = document
        .create_element("input")
        .map_err(|error| format!("创建文本输入控件失败：{error:?}"))?
        .dyn_into()
        .map_err(|_| "文本输入控件类型转换失败".to_string())?;
    input.set_type("text");
    input.set_autocomplete("off");
    input.set_spellcheck(false);
    input
        .set_attribute(
            "style",
            "position:fixed;display:none;z-index:2147483647;box-sizing:border-box;\
             margin:0;padding:0 10px;border:1px solid rgba(255,255,255,0.18);\
             border-radius:6px;background:rgba(18,18,18,0.88);color:#f3f3f3;\
             font:16px sans-serif;outline:none;",
        )
        .map_err(|error| format!("设置文本输入样式失败：{error:?}"))?;
    body.append_child(&input)
        .map_err(|error| format!("挂载文本输入控件失败：{error:?}"))?;

    *modal_text_input_slot().lock().unwrap() = Some(ModalTextInput {
        input: input.clone(),
        active_field: None,
    });
    Ok(input)
}

#[cfg(target_arch = "wasm32")]
pub fn sync_modal_text_input(
    field_id: &str,
    value: &str,
    rect: [f32; 4],
    request_focus: bool,
) -> Result<String, String> {
    let input = ensure_modal_text_input()?;
    let mut state = modal_text_input_slot().lock().unwrap();
    let state = state.as_mut().ok_or("文本输入控件状态不可用".to_string())?;

    let field_changed = state.active_field.as_deref() != Some(field_id);
    if field_changed {
        input.set_value(value);
        state.active_field = Some(field_id.to_string());
    }

    input
        .set_attribute(
            "style",
            &format!(
                "position:fixed;display:block;z-index:2147483647;box-sizing:border-box;\
                 left:{}px;top:{}px;width:{}px;height:{}px;margin:0;padding:0 10px;\
                 border:1px solid rgba(255,255,255,0.18);border-radius:6px;\
                 background:rgba(18,18,18,0.88);color:#f3f3f3;font:16px sans-serif;\
                 outline:none;",
                rect[0],
                rect[1],
                rect[2].max(32.0),
                rect[3].max(24.0)
            ),
        )
        .map_err(|error| format!("更新文本输入样式失败：{error:?}"))?;

    if field_changed || request_focus {
        let _ = input.focus();
    }

    Ok(input.value())
}

#[cfg(target_arch = "wasm32")]
pub fn hide_modal_text_input() -> Result<(), String> {
    let mut state = modal_text_input_slot().lock().unwrap();
    let Some(state) = state.as_mut() else {
        return Ok(());
    };
    state.active_field = None;
    state.input.set_value("");
    let _ = state.input.blur();
    state
        .input
        .set_attribute("style", "position:fixed;display:none;")
        .map_err(|error| format!("隐藏文本输入控件失败：{error:?}"))?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn get_asset_path(path: &'static str) -> String {
    // 在GitHub Pages上，资源路径需要加上/daboyi/前缀
    format!("/daboyi/{path}")
}

#[cfg(target_arch = "wasm32")]
pub async fn fetch_bytes(path: &'static str) -> Result<Vec<u8>, String> {
    let full_path = get_asset_path(path);
    let response = Request::get(&full_path)
        .send()
        .await
        .map_err(|error| format!("请求 {full_path} 失败：{error}"))?;
    if !response.ok() {
        return Err(format!("请求 {full_path} 失败：HTTP {}", response.status()));
    }
    response
        .binary()
        .await
        .map_err(|error| format!("读取 {full_path} 二进制内容失败：{error}"))
}

#[cfg(target_arch = "wasm32")]
pub async fn fetch_text(path: &'static str) -> Result<String, String> {
    let full_path = get_asset_path(path);
    let response = Request::get(&full_path)
        .send()
        .await
        .map_err(|error| format!("请求 {full_path} 失败：{error}"))?;
    if !response.ok() {
        return Err(format!("请求 {full_path} 失败：HTTP {}", response.status()));
    }
    response
        .text()
        .await
        .map_err(|error| format!("读取 {full_path} 文本内容失败：{error}"))
}

#[cfg(target_arch = "wasm32")]
pub fn take_uploaded_coloring() -> Option<Result<ColoringFile, String>> {
    coloring_load_slot().lock().unwrap().take()
}

#[cfg(target_arch = "wasm32")]
pub fn begin_coloring_upload() -> Result<(), String> {
    let window = web_sys::window().ok_or("浏览器窗口不可用".to_string())?;
    let document = window.document().ok_or("浏览器文档不可用".to_string())?;
    let body = document.body().ok_or("浏览器页面缺少 body".to_string())?;

    let input: HtmlInputElement = document
        .create_element("input")
        .map_err(|error| format!("创建文件输入控件失败：{error:?}"))?
        .dyn_into()
        .map_err(|_| "文件输入控件类型转换失败".to_string())?;
    input.set_type("file");
    input.set_accept(".json,application/json");
    input.set_hidden(true);

    body.append_child(&input)
        .map_err(|error| format!("挂载文件输入控件失败：{error:?}"))?;

    let body_for_cleanup = body.clone();
    let input_for_change = input.clone();
    let on_change = Closure::once(move |_event: web_sys::Event| {
        let maybe_file = input_for_change.files().and_then(|files| files.get(0));
        if let Some(file) = maybe_file {
            spawn_local(async move {
                let result = async {
                    let text = JsFuture::from(file.text())
                        .await
                        .map_err(|error| format!("读取 JSON 文件失败：{error:?}"))?
                        .as_string()
                        .ok_or("JSON 文件内容不是文本".to_string())?;
                    serde_json::from_str::<ColoringFile>(&text)
                        .map_err(|error| format!("解析着色文件失败：{error}"))
                }
                .await;
                *coloring_load_slot().lock().unwrap() = Some(result);
            });
        }
        let _ = body_for_cleanup.remove_child(&input_for_change);
    });

    input
        .add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref())
        .map_err(|error| format!("绑定文件选择事件失败：{error:?}"))?;
    on_change.forget();
    input.click();
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn download_text_file(filename: &str, contents: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or("浏览器窗口不可用".to_string())?;
    let document = window.document().ok_or("浏览器文档不可用".to_string())?;
    let body = document.body().ok_or("浏览器页面缺少 body".to_string())?;

    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(contents));
    let blob = Blob::new_with_str_sequence(&parts)
        .map_err(|error| format!("创建下载数据失败：{error:?}"))?;
    let url = Url::create_object_url_with_blob(&blob)
        .map_err(|error| format!("创建下载链接失败：{error:?}"))?;

    let anchor: HtmlAnchorElement = document
        .create_element("a")
        .map_err(|error| format!("创建下载链接节点失败：{error:?}"))?
        .dyn_into()
        .map_err(|_| "下载链接节点类型转换失败".to_string())?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.set_hidden(true);

    body.append_child(&anchor)
        .map_err(|error| format!("挂载下载链接失败：{error:?}"))?;
    anchor.click();
    let _ = body.remove_child(&anchor);
    let _ = Url::revoke_object_url(&url);
    Ok(())
}
