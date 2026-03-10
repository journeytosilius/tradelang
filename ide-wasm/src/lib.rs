use iced::widget::canvas::{self, Canvas, Path, Stroke};
use iced::widget::{button, column, container, row, scrollable, text, text_editor, text_input};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Task, Theme};
use serde::Deserialize;

const DEFAULT_SOURCE: &str = r#"interval 4h
source spot = binance.spot("BTCUSDT")
use spot 1d
use spot 1w

let fast = ema(spot.close, 13)
let slow = ema(spot.close, 89)
let daily_fast = ema(spot.1d.close, 30)
let daily_slow = ema(spot.1d.close, 80)
let weekly_fast = ema(spot.1w.close, 5)
let weekly_slow = ema(spot.1w.close, 13)

entry long = above(fast, slow) and above(daily_fast, daily_slow) and above(weekly_fast, weekly_slow)
exit long = below(fast, slow)

plot(fast - slow)
export trend_long_state = above(fast, slow)
"#;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone)]
enum Message {
    CatalogLoaded(Result<PublicDatasetCatalog, String>),
    ScriptEdited(text_editor::Action),
    CheckFinished {
        request_id: u64,
        result: Result<CheckResponse, String>,
    },
    RunBacktest,
    BacktestFinished(Result<BacktestResponse, String>),
    FromDateChanged(String),
    ToDateChanged(String),
}

#[derive(Debug)]
struct IdeApp {
    script: text_editor::Content,
    diagnostics: Vec<Diagnostic>,
    backtest: Option<BacktestResponse>,
    dataset: Option<PublicDataset>,
    from_input: String,
    to_input: String,
    status: String,
    next_check_request_id: u64,
    latest_check_request_id: u64,
    checking: bool,
    running_backtest: bool,
}

impl Default for IdeApp {
    fn default() -> Self {
        Self {
            script: text_editor::Content::with_text(DEFAULT_SOURCE),
            diagnostics: Vec::new(),
            backtest: None,
            dataset: None,
            from_input: String::new(),
            to_input: String::new(),
            status: "Loading curated dataset…".to_string(),
            next_check_request_id: 0,
            latest_check_request_id: 0,
            checking: false,
            running_backtest: false,
        }
    }
}

pub fn run() -> iced::Result {
    iced::application(
        || {
            let app = IdeApp::default();
            let task = Task::batch([
                Task::perform(fetch_dataset_catalog(), Message::CatalogLoaded),
                initial_check_task(DEFAULT_SOURCE.to_string(), 0),
            ]);
            (app, task)
        },
        update,
        view,
    )
    .title(app_title)
    .theme(app_theme)
    .window_size((1400.0, 920.0))
    .antialiasing(true)
    .run()
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() -> Result<(), wasm_bindgen::JsValue> {
    run().map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
}

fn update(state: &mut IdeApp, message: Message) -> Task<Message> {
    match message {
        Message::CatalogLoaded(result) => {
            match result {
                Ok(catalog) => {
                    if let Some(dataset) = catalog.datasets.into_iter().next() {
                        let (from_input, to_input) = default_window_for_dataset(&dataset);
                        state.status = format!(
                            "{} available from {} to {}",
                            dataset.display_name,
                            format_date_ms(dataset.from),
                            format_date_ms(dataset.to - DAY_MS)
                        );
                        state.from_input = from_input;
                        state.to_input = to_input;
                        state.dataset = Some(dataset);
                    } else {
                        state.status = "No curated dataset is available.".to_string();
                    }
                }
                Err(error) => {
                    state.status = error;
                }
            }
            Task::none()
        }
        Message::ScriptEdited(action) => {
            state.script.perform(action);
            state.checking = true;
            state.next_check_request_id += 1;
            let request_id = state.next_check_request_id;
            state.latest_check_request_id = request_id;
            initial_check_task(state.script.text(), request_id)
        }
        Message::CheckFinished { request_id, result } => {
            if request_id != state.latest_check_request_id {
                return Task::none();
            }
            state.checking = false;
            match result {
                Ok(response) => {
                    state.diagnostics = response.diagnostics;
                    if state.diagnostics.is_empty() {
                        state.status = if state.running_backtest {
                            "Running backtest…".to_string()
                        } else {
                            "Ready".to_string()
                        };
                    } else {
                        state.status =
                            format!("{} diagnostic(s) need attention", state.diagnostics.len());
                    }
                }
                Err(error) => {
                    state.status = error;
                }
            }
            Task::none()
        }
        Message::FromDateChanged(value) => {
            state.from_input = value;
            normalize_date_inputs(state);
            Task::none()
        }
        Message::ToDateChanged(value) => {
            state.to_input = value;
            normalize_date_inputs(state);
            Task::none()
        }
        Message::RunBacktest => {
            let Some(dataset) = state.dataset.clone() else {
                state.status = "No curated dataset is available.".to_string();
                return Task::none();
            };
            match selected_window(&dataset, &state.from_input, &state.to_input) {
                Ok(window) => {
                    state.running_backtest = true;
                    state.status = "Running backtest…".to_string();
                    Task::perform(
                        run_backtest_request(dataset.dataset_id, state.script.text(), window),
                        Message::BacktestFinished,
                    )
                }
                Err(error) => {
                    state.status = error;
                    Task::none()
                }
            }
        }
        Message::BacktestFinished(result) => {
            state.running_backtest = false;
            match result {
                Ok(response) => {
                    let range = format!(
                        "{} -> {}",
                        format_date_ms(response.dataset.from),
                        format_date_ms(response.dataset.to - DAY_MS)
                    );
                    state.status = format!("Backtest complete for {range}");
                    state.backtest = Some(response);
                }
                Err(error) => {
                    state.status = error;
                }
            }
            Task::none()
        }
    }
}

fn app_title(_state: &IdeApp) -> String {
    "PalmScript IDE".to_string()
}

fn app_theme(_state: &IdeApp) -> Theme {
    Theme::Light
}

fn view(state: &IdeApp) -> Element<'_, Message> {
    let toolbar = container(
        row![
            text("PalmScript IDE").size(26),
            row![
                date_field("From", &state.from_input, Message::FromDateChanged),
                date_field("To", &state.to_input, Message::ToDateChanged),
                button(text(if state.running_backtest {
                    "Running…"
                } else {
                    "Run Backtest"
                }))
                .style(primary_button_style)
                .padding([12, 18])
                .on_press_maybe((!state.running_backtest).then_some(Message::RunBacktest)),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            text(status_label(state))
                .size(16)
                .color(status_color(state)),
        ]
        .align_y(Alignment::Center)
        .spacing(20),
    )
    .style(panel_style)
    .padding(20);

    let editor = container(
        text_editor(&state.script)
            .placeholder("Write a PalmScript strategy")
            .font(iced::Font::MONOSPACE)
            .size(16)
            .on_action(Message::ScriptEdited)
            .height(Fill),
    )
    .height(Fill)
    .style(editor_panel_style)
    .padding(18);

    let diagnostics = diagnostics_panel(state);
    let summary = summary_panel(state);
    let chart = equity_panel(state);
    let trades = list_panel("Trades", render_trades(state));
    let orders = list_panel("Orders", render_orders(state));

    container(
        column![
            toolbar,
            row![
                container(editor).width(Length::FillPortion(3)).height(Fill),
                column![diagnostics, summary, chart, trades, orders]
                    .width(Length::FillPortion(2))
                    .spacing(16),
            ]
            .spacing(16)
            .height(Fill),
        ]
        .spacing(16)
        .padding(16),
    )
    .width(Fill)
    .height(Fill)
    .style(root_style)
    .into()
}

fn diagnostics_panel(state: &IdeApp) -> Element<'_, Message> {
    let content = if state.diagnostics.is_empty() {
        column![muted("No diagnostics.")]
    } else {
        state
            .diagnostics
            .iter()
            .fold(column!().spacing(12), |column, diagnostic| {
                column.push(
                    container(
                        column![
                            text(&diagnostic.message).size(15),
                            muted(format!(
                                "line {}, column {}",
                                diagnostic.range.start.line + 1,
                                diagnostic.range.start.character + 1
                            )),
                        ]
                        .spacing(4),
                    )
                    .style(diagnostic_item_style)
                    .padding(12),
                )
            })
    };

    panel(
        "Diagnostics",
        scrollable(content).height(Length::Fixed(180.0)).into(),
    )
}

fn summary_panel(state: &IdeApp) -> Element<'_, Message> {
    let content: Element<'_, Message> = match &state.backtest {
        Some(response) => {
            let summary = &response.result.summary;
            let cards = column![
                summary_card(
                    "Dataset",
                    format!(
                        "{} ({} -> {})",
                        response.dataset.display_name,
                        format_date_ms(response.dataset.from),
                        format_date_ms(response.dataset.to - DAY_MS)
                    ),
                    None
                ),
                row![
                    summary_card(
                        "Ending Equity",
                        format_number(summary.ending_equity, 2),
                        None
                    ),
                    summary_card("Trades", summary.trade_count.to_string(), None),
                ]
                .spacing(12),
                row![
                    summary_card(
                        "Total Return",
                        format_percent(summary.total_return * 100.0),
                        Some(number_color(summary.total_return))
                    ),
                    summary_card("Win Rate", format_percent(summary.win_rate * 100.0), None),
                ]
                .spacing(12),
                summary_card(
                    "Max Drawdown",
                    format_number(summary.max_drawdown, 2),
                    Some(number_color(-summary.max_drawdown.abs()))
                ),
            ]
            .spacing(12);
            cards.into()
        }
        None => muted("No run yet.").into(),
    };
    panel("Backtest Summary", content)
}

fn equity_panel(state: &IdeApp) -> Element<'_, Message> {
    let chart = Canvas::new(EquityChart {
        points: state
            .backtest
            .as_ref()
            .map(|response| response.result.equity_curve.clone())
            .unwrap_or_default(),
    })
    .width(Fill)
    .height(Length::Fixed(180.0));
    panel("Equity Curve", chart.into())
}

fn render_trades(state: &IdeApp) -> iced::widget::Column<'_, Message> {
    match &state.backtest {
        Some(response) if !response.result.trades.is_empty() => response
            .result
            .trades
            .iter()
            .take(50)
            .fold(column!().spacing(10), |column, trade| {
                column.push(
                    container(
                        column![
                            text(format!(
                                "{}  {} -> {}",
                                trade.side,
                                format_time_ms(trade.entry.time),
                                format_time_ms(trade.exit.time)
                            ))
                            .size(15),
                            muted(format!(
                                "entry {} / exit {} / pnl {}",
                                format_number(trade.entry.price, 2),
                                format_number(trade.exit.price, 2),
                                format_number(trade.realized_pnl, 2)
                            )),
                        ]
                        .spacing(4),
                    )
                    .style(list_item_style)
                    .padding(12),
                )
            }),
        _ => column![muted("No trades.")],
    }
}

fn render_orders(state: &IdeApp) -> iced::widget::Column<'_, Message> {
    match &state.backtest {
        Some(response) if !response.result.orders.is_empty() => response
            .result
            .orders
            .iter()
            .take(50)
            .fold(column!().spacing(10), |column, order| {
                column.push(
                    container(
                        column![
                            text(format!(
                                "{:?}  {:?} / {:?}",
                                order.role, order.kind, order.status
                            ))
                            .size(15),
                            muted(format!(
                                "placed {} / fill {}",
                                format_time_ms(order.placed_time),
                                order
                                    .fill_price
                                    .map(|value| format_number(value, 2))
                                    .unwrap_or_else(|| "NA".to_string())
                            )),
                        ]
                        .spacing(4),
                    )
                    .style(list_item_style)
                    .padding(12),
                )
            }),
        _ => column![muted("No orders.")],
    }
}

fn list_panel<'a>(
    title: &'static str,
    content: iced::widget::Column<'a, Message>,
) -> Element<'a, Message> {
    panel(
        title,
        scrollable(content).height(Length::Fixed(180.0)).into(),
    )
}

fn panel<'a>(title: &'static str, content: Element<'a, Message>) -> Element<'a, Message> {
    container(column![text(title).size(18), content].spacing(12))
        .style(panel_style)
        .padding(16)
        .into()
}

fn summary_card(
    label: &'static str,
    value: String,
    color: Option<Color>,
) -> iced::widget::Container<'static, Message> {
    let value = text(value)
        .size(20)
        .color(color.unwrap_or_else(|| Color::from_rgb8(0x18, 0x32, 0x47)));
    container(column![muted(label), value].spacing(8))
        .style(summary_card_style)
        .padding(14)
        .width(Fill)
}

fn date_field<'a>(
    label: &'static str,
    value: &'a str,
    on_input: fn(String) -> Message,
) -> Element<'a, Message> {
    container(
        column![
            muted(label),
            text_input("YYYY-MM-DD", value)
                .on_input(on_input)
                .padding(10)
                .width(Length::Fixed(140.0)),
        ]
        .spacing(6),
    )
    .into()
}

fn muted(content: impl Into<String>) -> iced::widget::Text<'static> {
    text(content.into())
        .size(13)
        .color(Color::from_rgb8(0x64, 0x7b, 0x92))
}

fn status_label(state: &IdeApp) -> String {
    if state.checking && !state.running_backtest {
        "Checking…".to_string()
    } else {
        state.status.clone()
    }
}

fn status_color(state: &IdeApp) -> Color {
    if state.diagnostics.is_empty() {
        Color::from_rgb8(0x15, 0x6f, 0xbe)
    } else {
        Color::from_rgb8(0xb8, 0x54, 0x29)
    }
}

fn default_window_for_dataset(dataset: &PublicDataset) -> (String, String) {
    let default_from = (dataset.to - (365 * DAY_MS)).max(dataset.from);
    (
        format_date_ms(default_from),
        format_date_ms(dataset.to - DAY_MS),
    )
}

fn normalize_date_inputs(state: &mut IdeApp) {
    if state.from_input.is_empty() || state.to_input.is_empty() {
        return;
    }
    if state.from_input > state.to_input {
        state.to_input = state.from_input.clone();
    }
}

fn selected_window(
    dataset: &PublicDataset,
    from_input: &str,
    to_input: &str,
) -> Result<SelectedWindow, String> {
    let from_ms = parse_date(from_input).ok_or_else(|| "Enter a valid From date.".to_string())?;
    let to_ms = parse_date(to_input)
        .map(|value| value + DAY_MS)
        .ok_or_else(|| "Enter a valid To date.".to_string())?;

    if from_ms >= to_ms {
        return Err("The selected range must span at least one day.".to_string());
    }
    if from_ms < dataset.from || to_ms > dataset.to {
        return Err(format!(
            "The curated dataset only supports {} through {}.",
            format_date_ms(dataset.from),
            format_date_ms(dataset.to - DAY_MS)
        ));
    }

    Ok(SelectedWindow { from_ms, to_ms })
}

fn parse_date(value: &str) -> Option<i64> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    Some(days * DAY_MS)
}

fn format_date_ms(ms: i64) -> String {
    let days = ms.div_euclid(DAY_MS);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn format_time_ms(ms: f64) -> String {
    format_date_ms(ms as i64)
}

fn format_number(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return "NA".to_string();
    }
    format!("{value:.digits$}")
}

fn format_percent(value: f64) -> String {
    if !value.is_finite() {
        return "NA".to_string();
    }
    format!("{value:.2}%")
}

fn number_color(value: f64) -> Color {
    if value > 0.0 {
        Color::from_rgb8(0x1f, 0x84, 0x59)
    } else if value < 0.0 {
        Color::from_rgb8(0xbf, 0x53, 0x2f)
    } else {
        Color::from_rgb8(0x18, 0x32, 0x47)
    }
}

fn root_style(_theme: &Theme) -> container::Style {
    container::Style::default().background(Background::Color(Color::from_rgb8(0xf4, 0xf8, 0xfc)))
}

fn panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(0xff, 0xff, 0xff))),
        border: Border {
            color: Color::from_rgb8(0xc9, 0xda, 0xeb),
            width: 1.0,
            radius: 16.0.into(),
        },
        text_color: Some(Color::from_rgb8(0x18, 0x32, 0x47)),
        ..container::Style::default()
    }
}

fn editor_panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(0xfc, 0xfd, 0xff))),
        border: Border {
            color: Color::from_rgb8(0xc9, 0xda, 0xeb),
            width: 1.0,
            radius: 18.0.into(),
        },
        ..container::Style::default()
    }
}

fn summary_card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(0xf8, 0xfb, 0xfe))),
        border: Border {
            color: Color::from_rgb8(0xd6, 0xe5, 0xf3),
            width: 1.0,
            radius: 14.0.into(),
        },
        ..container::Style::default()
    }
}

fn diagnostic_item_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(0xff, 0xf7, 0xf1))),
        border: Border {
            color: Color::from_rgb8(0xf2, 0xc8, 0xa5),
            width: 1.0,
            radius: 12.0.into(),
        },
        ..container::Style::default()
    }
}

fn list_item_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(0xf8, 0xfb, 0xfe))),
        border: Border {
            color: Color::from_rgb8(0xde, 0xea, 0xf6),
            width: 1.0,
            radius: 12.0.into(),
        },
        ..container::Style::default()
    }
}

fn primary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgb8(0x0f, 0x5f, 0x9f),
        button::Status::Disabled => Color::from_rgba8(0x15, 0x6f, 0xbe, 0.35),
        _ => Color::from_rgb8(0x15, 0x6f, 0xbe),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: Color::WHITE,
        border: Border {
            color: background,
            width: 1.0,
            radius: 14.0.into(),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba8(0x15, 0x6f, 0xbe, 0.2),
            offset: iced::Vector::new(0.0, 6.0),
            blur_radius: 16.0,
        },
        snap: false,
    }
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    let day_max = days_in_month(year, month)?;
    if day == 0 || day > day_max {
        return None;
    }
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - (era * 400);
    let month = month as i32;
    let day = day as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) as i64)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i32::from(month <= 2);
    (year, month as u32, day as u32)
}

fn days_in_month(year: i32, month: u32) -> Option<u32> {
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return None,
    };
    Some(days)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn initial_check_task(script: String, request_id: u64) -> Task<Message> {
    Task::perform(
        check_script_request(script, request_id),
        |(request_id, result)| Message::CheckFinished { request_id, result },
    )
}

async fn check_script_request(
    script: String,
    request_id: u64,
) -> (u64, Result<CheckResponse, String>) {
    (
        request_id,
        post_json("api/check", &CheckRequest { script }).await,
    )
}

async fn fetch_dataset_catalog() -> Result<PublicDatasetCatalog, String> {
    get_json("api/datasets").await
}

async fn run_backtest_request(
    dataset_id: PublicDatasetId,
    script: String,
    window: SelectedWindow,
) -> Result<BacktestResponse, String> {
    post_json(
        "api/backtest",
        &BacktestRequest {
            script,
            dataset_id,
            from_ms: window.from_ms,
            to_ms: window.to_ms,
        },
    )
    .await
}

#[cfg(target_arch = "wasm32")]
async fn get_json<T>(path: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    use gloo_net::http::Request;

    let response = Request::get(path)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    parse_response(response).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn get_json<T>(_path: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    Err("The PalmScript IDE web frontend only runs in a browser.".to_string())
}

#[cfg(target_arch = "wasm32")]
async fn post_json<T, B>(path: &str, body: &B) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
    B: serde::Serialize,
{
    use gloo_net::http::Request;

    let request = Request::post(path)
        .header("content-type", "application/json")
        .header("x-palmscript-session", &browser_session_id())
        .json(body)
        .map_err(|error| error.to_string())?;
    let response = request.send().await.map_err(|error| error.to_string())?;
    parse_response(response).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn post_json<T, B>(_path: &str, _body: &B) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
    B: serde::Serialize,
{
    Err("The PalmScript IDE web frontend only runs in a browser.".to_string())
}

#[cfg(target_arch = "wasm32")]
async fn parse_response<T>(response: gloo_net::http::Response) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let status = response.status();
    let body = response.text().await.map_err(|error| error.to_string())?;
    if (200..300).contains(&status) {
        serde_json::from_str(&body).map_err(|error| error.to_string())
    } else {
        let api_error: ApiErrorBody =
            serde_json::from_str(&body).unwrap_or_else(|_| ApiErrorBody { error: body });
        Err(api_error.error)
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_session_id() -> String {
    web_sys::window()
        .and_then(|window| window.crypto().ok())
        .map(|crypto| crypto.random_uuid())
        .unwrap_or_else(|| format!("session-{}", js_sys::Date::now() as u64))
}

struct EquityChart {
    points: Vec<EquityPoint>,
}

impl<Message> canvas::Program<Message> for EquityChart {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let background = Path::rectangle(iced::Point::ORIGIN, bounds.size());
        frame.fill(&background, Color::from_rgb8(0xf8, 0xfb, 0xfe));

        if self.points.len() >= 2 {
            let min = self
                .points
                .iter()
                .map(|point| point.equity)
                .fold(f64::INFINITY, f64::min);
            let max = self
                .points
                .iter()
                .map(|point| point.equity)
                .fold(f64::NEG_INFINITY, f64::max);
            let span = (max - min).max(1.0);

            let path = Path::new(|builder| {
                for (index, point) in self.points.iter().enumerate() {
                    let x = if self.points.len() == 1 {
                        0.0
                    } else {
                        (index as f32 / (self.points.len() - 1) as f32) * bounds.width
                    };
                    let y = bounds.height
                        - 12.0
                        - (((point.equity - min) / span) as f32 * (bounds.height - 24.0));
                    if index == 0 {
                        builder.move_to(iced::Point::new(x, y));
                    } else {
                        builder.line_to(iced::Point::new(x, y));
                    }
                }
            });
            frame.stroke(
                &path,
                Stroke::default()
                    .with_width(3.0)
                    .with_color(Color::from_rgb8(0x15, 0x6f, 0xbe)),
            );
        }

        vec![frame.into_geometry()]
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Deserialize)]
struct ApiErrorBody {
    error: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PublicDatasetCatalog {
    datasets: Vec<PublicDataset>,
}

#[derive(Debug, Clone, Deserialize)]
struct PublicDataset {
    dataset_id: PublicDatasetId,
    display_name: String,
    from: i64,
    to: i64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum PublicDatasetId {
    BtcusdtBinanceSpot4h4y,
}

#[derive(Debug, Clone, serde::Serialize)]
struct CheckRequest {
    script: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CheckResponse {
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct BacktestRequest {
    script: String,
    dataset_id: PublicDatasetId,
    from_ms: i64,
    to_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct BacktestResponse {
    dataset: PublicDataset,
    result: BacktestResult,
}

#[derive(Debug, Clone, Deserialize)]
struct BacktestResult {
    orders: Vec<OrderRecord>,
    trades: Vec<Trade>,
    equity_curve: Vec<EquityPoint>,
    summary: BacktestSummary,
}

#[derive(Debug, Clone, Deserialize)]
struct BacktestSummary {
    ending_equity: f64,
    total_return: f64,
    trade_count: usize,
    win_rate: f64,
    max_drawdown: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct Trade {
    side: String,
    entry: TradeFill,
    exit: TradeFill,
    realized_pnl: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct TradeFill {
    time: f64,
    price: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct OrderRecord {
    role: String,
    kind: String,
    status: String,
    placed_time: f64,
    fill_price: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct EquityPoint {
    equity: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct Diagnostic {
    message: String,
    range: DiagnosticRange,
}

#[derive(Debug, Clone, Deserialize)]
struct DiagnosticRange {
    start: DiagnosticPosition,
}

#[derive(Debug, Clone, Deserialize)]
struct DiagnosticPosition {
    line: usize,
    character: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedWindow {
    from_ms: i64,
    to_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::{
        format_date_ms, format_number, format_percent, selected_window, PublicDataset,
        PublicDatasetId,
    };

    fn dataset() -> PublicDataset {
        PublicDataset {
            dataset_id: PublicDatasetId::BtcusdtBinanceSpot4h4y,
            display_name: "BTC".to_string(),
            from: 1_700_000_000_000,
            to: 1_800_000_000_000,
        }
    }

    #[test]
    fn selected_window_accepts_curated_range() {
        let window = selected_window(&dataset(), "2023-11-20", "2024-01-05").expect("window");
        assert!(window.from_ms < window.to_ms);
    }

    #[test]
    fn selected_window_rejects_out_of_bounds_range() {
        let err = selected_window(&dataset(), "2021-01-01", "2021-01-10").expect_err("range");
        assert!(err.contains("curated dataset"));
    }

    #[test]
    fn summary_formatters_handle_numbers() {
        assert_eq!(format_number(12.3456, 2), "12.35");
        assert_eq!(format_percent(18.234), "18.23%");
        assert_eq!(format_date_ms(1_700_000_000_000), "2023-11-14");
    }
}
