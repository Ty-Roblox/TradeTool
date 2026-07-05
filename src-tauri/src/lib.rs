mod capture;
mod filters;
mod models;
mod parser;
mod stat_patterns;
mod trade;

use tauri::Emitter;

#[tauri::command]
fn parse_item_text(raw_text: String) -> Result<models::CaptureResponse, String> {
    build_capture_response(raw_text)
}

#[tauri::command]
fn capture_item_now(app: tauri::AppHandle) -> Result<models::CaptureResponse, String> {
    let raw_text = capture::capture_item_text()?;
    let response = build_capture_response(raw_text)?;
    let _ = app.emit("item_captured", &response);
    Ok(response)
}

#[tauri::command]
async fn search_trade(
    request: models::SearchTradeRequest,
) -> Result<models::TradeSearchResponse, String> {
    let item = match request.raw_text.as_deref().map(str::trim) {
        Some(raw_text) if !raw_text.is_empty() => parser::parse_item_text(raw_text)?,
        _ => models::CapturedItem::empty(),
    };

    trade::search_trade(&request.league, &item, &request.selected_filter_ids).await
}

#[tauri::command]
fn open_trade_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let url = trade::validate_trade_url(&url)?;
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| format!("Opening the trade result page failed: {error}"))
}

fn build_capture_response(raw_text: String) -> Result<models::CaptureResponse, String> {
    let item = parser::parse_item_text(&raw_text)?;
    let filter_groups = filters::generate_filter_groups(&item);
    let diagnostics = filters::generate_capture_diagnostics(&filter_groups);

    Ok(models::CaptureResponse {
        hotkey: capture::CAPTURE_HOTKEY.to_string(),
        item,
        filter_groups,
        diagnostics,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Shortcut, ShortcutState,
                };

                let shortcut = Shortcut::new(None, Code::F8);
                let handler_shortcut = shortcut.clone();

                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |app, shortcut, event| {
                            if shortcut == &handler_shortcut && event.state() == ShortcutState::Pressed {
                                let app = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    match capture::capture_item_text().and_then(build_capture_response) {
                                        Ok(response) => {
                                            let _ = app.emit("item_captured", response);
                                        }
                                        Err(error) => {
                                            let _ = app.emit("capture_error", error);
                                        }
                                    }
                                });
                            }
                        })
                        .build(),
                )?;
                app.global_shortcut().register(shortcut)?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            capture_item_now,
            parse_item_text,
            search_trade,
            open_trade_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
