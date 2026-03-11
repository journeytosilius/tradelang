use iced::widget::canvas::{self, Canvas, Path, Stroke};
use iced::widget::text_editor::{
    Action as EditorAction, Binding as EditorBinding, KeyPress as EditorKeyPress,
};
use iced::widget::{button, column, container, row, scrollable, text, text_editor};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Task, Theme};
use iced_aw::drop_down::{Alignment as DropDownAlignment, Offset as DropDownOffset};
use iced_aw::{date_picker::Date, DropDown, ICED_AW_FONT_BYTES};
#[cfg(target_arch = "wasm32")]
use js_sys::{Function, Promise, Reflect};
use serde::Deserialize;
use std::ops::Range;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

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
const DATE_BUTTON_WIDTH: f32 = 140.0;
const DATE_BUTTON_HEIGHT: f32 = 40.0;
const DATE_FIELD_SPACING: f32 = 6.0;
const DATE_PICKER_WIDTH: f32 = 272.0;
const DAY_CELL_SIZE: f32 = 32.0;
const HEADER_BRAND_WIDTH: f32 = 240.0;
const HEADER_BRAND_HEIGHT: f32 = 80.0;

#[derive(Debug, Clone)]
enum Message {
    CatalogLoaded(Result<PublicDatasetCatalog, String>),
    ScriptEdited(text_editor::Action),
    CheckFinished {
        request_id: u64,
        result: Result<CheckResponse, String>,
    },
    #[cfg(target_arch = "wasm32")]
    ClipboardWriteFinished(Result<(), String>),
    #[cfg(target_arch = "wasm32")]
    ClipboardReadFinished(Result<String, String>),
    #[cfg(target_arch = "wasm32")]
    CopySelection,
    #[cfg(target_arch = "wasm32")]
    CutSelection,
    #[cfg(target_arch = "wasm32")]
    PasteFromClipboard,
    RunBacktest,
    BacktestFinished(Result<BacktestResponse, String>),
    ChooseFromDate,
    DismissFromDatePicker,
    ShiftFromMonth(i32),
    SubmitFromDate(Date),
    ChooseToDate,
    DismissToDatePicker,
    ShiftToMonth(i32),
    SubmitToDate(Date),
}

#[derive(Debug)]
struct IdeApp {
    script: text_editor::Content,
    diagnostics: Vec<Diagnostic>,
    highlight_settings: EditorHighlightSettings,
    backtest: Option<BacktestResponse>,
    dataset: Option<PublicDataset>,
    from_date: Date,
    to_date: Date,
    from_picker_month: CalendarMonth,
    to_picker_month: CalendarMonth,
    show_from_picker: bool,
    show_to_picker: bool,
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
            highlight_settings: EditorHighlightSettings::default(),
            backtest: None,
            dataset: None,
            from_date: Date::default(),
            to_date: Date::default(),
            from_picker_month: CalendarMonth::from_date(Date::default()),
            to_picker_month: CalendarMonth::from_date(Date::default()),
            show_from_picker: false,
            show_to_picker: false,
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
    .font(ICED_AW_FONT_BYTES)
    .window_size((1400.0, 920.0))
    .antialiasing(true)
    .run()
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() -> Result<(), wasm_bindgen::JsValue> {
    run().map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
}

fn apply_editor_action(state: &mut IdeApp, action: EditorAction) -> Task<Message> {
    state.script.perform(action);
    state.checking = true;
    state.next_check_request_id += 1;
    let request_id = state.next_check_request_id;
    state.latest_check_request_id = request_id;
    initial_check_task(state.script.text(), request_id)
}

fn update(state: &mut IdeApp, message: Message) -> Task<Message> {
    match message {
        Message::CatalogLoaded(result) => {
            match result {
                Ok(catalog) => {
                    if let Some(dataset) = catalog.datasets.into_iter().next() {
                        let (from_date, to_date) = default_window_for_dataset(&dataset);
                        state.status = format!(
                            "{} available from {} to {}",
                            dataset.display_name,
                            format_date_ms(dataset.from),
                            format_date_ms(dataset.to - DAY_MS)
                        );
                        state.from_date = from_date;
                        state.to_date = to_date;
                        state.from_picker_month = CalendarMonth::from_date(from_date);
                        state.to_picker_month = CalendarMonth::from_date(to_date);
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
        Message::ScriptEdited(action) => apply_editor_action(state, action),
        Message::CheckFinished { request_id, result } => {
            if request_id != state.latest_check_request_id {
                return Task::none();
            }
            state.checking = false;
            match result {
                Ok(response) => {
                    state.diagnostics = response.diagnostics;
                    state.highlight_settings = EditorHighlightSettings::from_source(
                        &state.script.text(),
                        &response.highlights,
                    );
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
        #[cfg(target_arch = "wasm32")]
        Message::ClipboardWriteFinished(result) => {
            if let Err(error) = result {
                state.status = error;
            }
            Task::none()
        }
        #[cfg(target_arch = "wasm32")]
        Message::ClipboardReadFinished(result) => match result {
            Ok(text) => apply_editor_action(
                state,
                EditorAction::Edit(text_editor::Edit::Paste(std::sync::Arc::new(text))),
            ),
            Err(error) => {
                state.status = error;
                Task::none()
            }
        },
        #[cfg(target_arch = "wasm32")]
        Message::CopySelection => {
            let Some(selection) = state.script.selection() else {
                return Task::none();
            };
            start_browser_clipboard_write(selection)
        }
        #[cfg(target_arch = "wasm32")]
        Message::CutSelection => {
            let Some(selection) = state.script.selection() else {
                return Task::none();
            };
            let delete_task =
                apply_editor_action(state, EditorAction::Edit(text_editor::Edit::Delete));
            Task::batch([delete_task, start_browser_clipboard_write(selection)])
        }
        #[cfg(target_arch = "wasm32")]
        Message::PasteFromClipboard => start_browser_clipboard_read(),
        Message::ChooseFromDate => {
            let opening = !state.show_from_picker;
            state.show_from_picker = opening;
            state.show_to_picker = false;
            if opening {
                state.from_picker_month = CalendarMonth::from_date(state.from_date);
            }
            Task::none()
        }
        Message::DismissFromDatePicker => {
            state.show_from_picker = false;
            Task::none()
        }
        Message::ShiftFromMonth(delta) => {
            state.from_picker_month = state.from_picker_month.shift(delta);
            Task::none()
        }
        Message::SubmitFromDate(date) => {
            state.from_date = date;
            state.show_from_picker = false;
            normalize_dates(state, PickedDate::From);
            state.from_picker_month = CalendarMonth::from_date(state.from_date);
            state.to_picker_month = CalendarMonth::from_date(state.to_date);
            Task::none()
        }
        Message::ChooseToDate => {
            let opening = !state.show_to_picker;
            state.show_to_picker = opening;
            state.show_from_picker = false;
            if opening {
                state.to_picker_month = CalendarMonth::from_date(state.to_date);
            }
            Task::none()
        }
        Message::DismissToDatePicker => {
            state.show_to_picker = false;
            Task::none()
        }
        Message::ShiftToMonth(delta) => {
            state.to_picker_month = state.to_picker_month.shift(delta);
            Task::none()
        }
        Message::SubmitToDate(date) => {
            state.to_date = date;
            state.show_to_picker = false;
            normalize_dates(state, PickedDate::To);
            state.from_picker_month = CalendarMonth::from_date(state.from_date);
            state.to_picker_month = CalendarMonth::from_date(state.to_date);
            Task::none()
        }
        Message::RunBacktest => {
            let Some(dataset) = state.dataset.clone() else {
                state.status = "No curated dataset is available.".to_string();
                return Task::none();
            };
            match selected_window(&dataset, state.from_date, state.to_date) {
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
            container(text(""))
                .width(Length::Fixed(HEADER_BRAND_WIDTH))
                .height(Length::Fixed(HEADER_BRAND_HEIGHT)),
            row![
                date_field(DateFieldProps {
                    label: "From",
                    value: state.from_date,
                    picker_month: state.from_picker_month,
                    show_picker: state.show_from_picker,
                    on_toggle: Message::ChooseFromDate,
                    on_dismiss: Message::DismissFromDatePicker,
                    on_prev_month: Message::ShiftFromMonth(-1),
                    on_next_month: Message::ShiftFromMonth(1),
                    on_submit: Message::SubmitFromDate,
                }),
                date_field(DateFieldProps {
                    label: "To",
                    value: state.to_date,
                    picker_month: state.to_picker_month,
                    show_picker: state.show_to_picker,
                    on_toggle: Message::ChooseToDate,
                    on_dismiss: Message::DismissToDatePicker,
                    on_prev_month: Message::ShiftToMonth(-1),
                    on_next_month: Message::ShiftToMonth(1),
                    on_submit: Message::SubmitToDate,
                }),
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
            .highlight_with::<PalmHighlighter>(
                state.highlight_settings.clone(),
                editor_highlight_format,
            )
            .key_binding(editor_key_binding)
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

struct DateFieldProps {
    label: &'static str,
    value: Date,
    picker_month: CalendarMonth,
    show_picker: bool,
    on_toggle: Message,
    on_dismiss: Message,
    on_prev_month: Message,
    on_next_month: Message,
    on_submit: fn(Date) -> Message,
}

fn date_field<'a>(props: DateFieldProps) -> Element<'a, Message> {
    let trigger = column![
        muted(props.label),
        button(
            text(props.value.to_string())
                .width(Length::Fill)
                .height(Length::Fill)
                .center()
        )
        .padding(0)
        .width(Length::Fixed(DATE_BUTTON_WIDTH))
        .height(Length::Fixed(DATE_BUTTON_HEIGHT))
        .style(date_button_style)
        .on_press(props.on_toggle),
    ]
    .spacing(DATE_FIELD_SPACING);

    DropDown::new(
        trigger,
        calendar_picker(
            props.picker_month,
            props.value,
            props.on_prev_month,
            props.on_next_month,
            props.on_submit,
        ),
        props.show_picker,
    )
    .alignment(DropDownAlignment::BottomStart)
    .offset(DropDownOffset::new(0.0, DATE_FIELD_SPACING))
    .width(Length::Fixed(DATE_PICKER_WIDTH))
    .on_dismiss(props.on_dismiss)
    .into()
}

fn calendar_picker<'a>(
    month: CalendarMonth,
    selected: Date,
    on_prev_month: Message,
    on_next_month: Message,
    on_submit: fn(Date) -> Message,
) -> Element<'a, Message> {
    let header = row![
        button(text("<").width(Length::Fill).height(Length::Fill).center())
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(32.0))
            .padding(0)
            .style(calendar_nav_button_style)
            .on_press(on_prev_month),
        text(month.label())
            .size(15)
            .width(Length::Fill)
            .height(Length::Fixed(32.0))
            .center(),
        button(text(">").width(Length::Fill).height(Length::Fill).center())
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(32.0))
            .padding(0)
            .style(calendar_nav_button_style)
            .on_press(on_next_month),
    ]
    .align_y(Alignment::Center)
    .spacing(8);

    let weekdays = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
        .into_iter()
        .fold(row!().spacing(4), |row, label| {
            row.push(muted(label).width(Length::Fixed(DAY_CELL_SIZE)).center())
        });

    let first_weekday = weekday_index(month.year, month.month, 1);
    let total_days =
        days_in_month(month.year, month.month).expect("calendar month should be valid");
    let mut day = 1;
    let mut weeks = column![weekdays].spacing(4);

    for week in 0..6 {
        let mut row_days = row!().spacing(4);
        for weekday in 0..7 {
            let slot = week * 7 + weekday;
            let cell = if slot < first_weekday as usize || day > total_days {
                blank_day_cell()
            } else {
                let date = Date::from_ymd(month.year, month.month, day);
                let is_selected = same_date(date, selected);
                let label = day.to_string();
                day += 1;
                button(
                    text(label)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .center(),
                )
                .width(Length::Fixed(DAY_CELL_SIZE))
                .height(Length::Fixed(DAY_CELL_SIZE))
                .padding(0)
                .style(if is_selected {
                    selected_day_button_style
                } else {
                    calendar_day_button_style
                })
                .on_press(on_submit(date))
                .into()
            };
            row_days = row_days.push(cell);
        }
        weeks = weeks.push(row_days);
    }

    container(column![header, weeks].spacing(10))
        .width(Length::Fixed(DATE_PICKER_WIDTH))
        .style(calendar_picker_style)
        .padding(12)
        .into()
}

fn editor_key_binding(key_press: EditorKeyPress) -> Option<EditorBinding<Message>> {
    #[cfg(target_arch = "wasm32")]
    {
        if matches!(
            key_press.status,
            iced::widget::text_editor::Status::Focused { .. }
        ) {
            match key_press.key.to_latin(key_press.physical_key) {
                Some('c') if key_press.modifiers.command() => {
                    return Some(EditorBinding::Custom(Message::CopySelection));
                }
                Some('x') if key_press.modifiers.command() => {
                    return Some(EditorBinding::Custom(Message::CutSelection));
                }
                Some('v') if key_press.modifiers.command() && !key_press.modifiers.alt() => {
                    return Some(EditorBinding::Custom(Message::PasteFromClipboard));
                }
                _ => {}
            }
        }
    }

    EditorBinding::from_key_press(key_press)
}

fn editor_highlight_format(
    kind: &HighlightKind,
    _theme: &Theme,
) -> iced_core::text::highlighter::Format<iced::Font> {
    iced_core::text::highlighter::Format {
        color: Some(match kind {
            HighlightKind::Keyword => Color::from_rgb8(0x15, 0x6f, 0xbe),
            HighlightKind::String => Color::from_rgb8(0x9b, 0x53, 0x18),
            HighlightKind::Number => Color::from_rgb8(0x0f, 0x78, 0x6e),
            HighlightKind::Function => Color::from_rgb8(0x1c, 0x55, 0x94),
            HighlightKind::Variable => Color::from_rgb8(0x18, 0x32, 0x47),
            HighlightKind::Parameter => Color::from_rgb8(0x7f, 0x46, 0xb2),
            HighlightKind::Namespace => Color::from_rgb8(0x6b, 0x57, 0x1c),
            HighlightKind::Type => Color::from_rgb8(0xb8, 0x54, 0x29),
        }),
        font: None,
    }
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

fn default_window_for_dataset(dataset: &PublicDataset) -> (Date, Date) {
    let default_from = (dataset.to - (365 * DAY_MS)).max(dataset.from);
    (
        date_from_ms(default_from),
        date_from_ms(dataset.to - DAY_MS),
    )
}

fn normalize_dates(state: &mut IdeApp, picked: PickedDate) {
    if date_to_ms(state.from_date) <= date_to_ms(state.to_date) {
        return;
    }
    match picked {
        PickedDate::From => state.to_date = state.from_date,
        PickedDate::To => state.from_date = state.to_date,
    }
}

fn selected_window(
    dataset: &PublicDataset,
    from_date: Date,
    to_date: Date,
) -> Result<SelectedWindow, String> {
    let from_ms = date_to_ms(from_date);
    let to_ms = date_to_ms(to_date) + DAY_MS;

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

fn date_to_ms(date: Date) -> i64 {
    days_from_civil(date.year, date.month, date.day).expect("picker dates should be valid") * DAY_MS
}

fn date_from_ms(ms: i64) -> Date {
    let days = ms.div_euclid(DAY_MS);
    let (year, month, day) = civil_from_days(days);
    Date::from_ymd(year, month, day)
}

fn format_date_ms(ms: i64) -> String {
    date_from_ms(ms).to_string()
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

fn line_start_offsets(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, ch) in source.char_indices() {
        if ch == '\n' {
            starts.push(index + ch.len_utf8());
        }
    }
    starts
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

fn date_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let border_color = match status {
        button::Status::Hovered => Color::from_rgb8(0x9d, 0xc5, 0xea),
        _ => Color::from_rgb8(0xc9, 0xda, 0xeb),
    };
    button::Style {
        background: Some(Background::Color(Color::from_rgb8(0xff, 0xff, 0xff))),
        text_color: Color::from_rgb8(0x18, 0x32, 0x47),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    }
}

fn calendar_picker_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(0xff, 0xff, 0xff))),
        border: Border {
            color: Color::from_rgb8(0xc9, 0xda, 0xeb),
            width: 1.0,
            radius: 14.0.into(),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba8(0x18, 0x32, 0x47, 0.08),
            offset: iced::Vector::new(0.0, 8.0),
            blur_radius: 18.0,
        },
        ..container::Style::default()
    }
}

fn calendar_nav_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgb8(0xe6, 0xf0, 0xfa),
        _ => Color::from_rgb8(0xf5, 0xf9, 0xfd),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: Color::from_rgb8(0x18, 0x32, 0x47),
        border: Border {
            color: Color::from_rgb8(0xd6, 0xe5, 0xf3),
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    }
}

fn calendar_day_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgb8(0xec, 0xf4, 0xfc),
        _ => Color::from_rgb8(0xff, 0xff, 0xff),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: Color::from_rgb8(0x18, 0x32, 0x47),
        border: Border {
            color: Color::from_rgb8(0xde, 0xea, 0xf6),
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    }
}

fn selected_day_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgb8(0x0f, 0x5f, 0x9f),
        _ => Color::from_rgb8(0x15, 0x6f, 0xbe),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: Color::WHITE,
        border: Border {
            color: background,
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    }
}

fn blank_day_cell<'a>() -> Element<'a, Message> {
    container(text(" ").width(Length::Fill).height(Length::Fill).center())
        .width(Length::Fixed(DAY_CELL_SIZE))
        .height(Length::Fixed(DAY_CELL_SIZE))
        .into()
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

#[cfg(target_arch = "wasm32")]
fn start_browser_clipboard_write(contents: String) -> Task<Message> {
    let Some(window) = web_sys::window() else {
        return Task::done(Message::ClipboardWriteFinished(Err(
            "Clipboard is unavailable in this browser context.".to_string(),
        )));
    };
    let promise = match clipboard_text_promise(&window, "writeText", Some(contents.as_str())) {
        Ok(promise) => promise,
        Err(error) => return Task::done(Message::ClipboardWriteFinished(Err(error))),
    };

    Task::perform(
        async move {
            wasm_bindgen_futures::JsFuture::from(promise)
                .await
                .map(|_| ())
                .map_err(|error| format!("Failed to write to the browser clipboard: {error:?}"))
        },
        Message::ClipboardWriteFinished,
    )
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
fn start_browser_clipboard_write(_contents: String) -> Task<Message> {
    Task::none()
}

#[cfg(target_arch = "wasm32")]
fn start_browser_clipboard_read() -> Task<Message> {
    let Some(window) = web_sys::window() else {
        return Task::done(Message::ClipboardReadFinished(Err(
            "Clipboard is unavailable in this browser context.".to_string(),
        )));
    };
    let promise = match clipboard_text_promise(&window, "readText", None) {
        Ok(promise) => promise,
        Err(error) => return Task::done(Message::ClipboardReadFinished(Err(error))),
    };

    Task::perform(
        async move {
            let value = wasm_bindgen_futures::JsFuture::from(promise)
                .await
                .map_err(|error| format!("Failed to read from the browser clipboard: {error:?}"))?;
            value
                .as_string()
                .ok_or_else(|| "The browser clipboard did not return text.".to_string())
        },
        Message::ClipboardReadFinished,
    )
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
fn start_browser_clipboard_read() -> Task<Message> {
    Task::none()
}

#[cfg(target_arch = "wasm32")]
fn clipboard_text_promise(
    window: &web_sys::Window,
    method_name: &str,
    arg: Option<&str>,
) -> Result<Promise, String> {
    let navigator = window.navigator();
    let clipboard = Reflect::get(&navigator, &wasm_bindgen::JsValue::from_str("clipboard"))
        .map_err(|error| format!("Clipboard is unavailable in this browser context: {error:?}"))?;
    let method = Reflect::get(&clipboard, &wasm_bindgen::JsValue::from_str(method_name))
        .map_err(|error| format!("Clipboard method `{method_name}` is unavailable: {error:?}"))?
        .dyn_into::<Function>()
        .map_err(|_| format!("Clipboard method `{method_name}` is not callable."))?;
    let result = match arg {
        Some(value) => method.call1(&clipboard, &wasm_bindgen::JsValue::from_str(value)),
        None => method.call0(&clipboard),
    }
    .map_err(|error| format!("Clipboard method `{method_name}` failed to start: {error:?}"))?;
    result
        .dyn_into::<Promise>()
        .map_err(|_| format!("Clipboard method `{method_name}` did not return a promise."))
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
    highlights: Vec<HighlightToken>,
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct HighlightToken {
    span: HighlightSpan,
    kind: HighlightKind,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct HighlightSpan {
    start: HighlightPosition,
    end: HighlightPosition,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct HighlightPosition {
    offset: usize,
    line: usize,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum HighlightKind {
    Keyword,
    String,
    Number,
    Function,
    Variable,
    Parameter,
    Namespace,
    Type,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct EditorHighlightSettings {
    lines: Vec<Vec<LineHighlight>>,
}

#[derive(Debug, Clone, PartialEq)]
struct LineHighlight {
    range: Range<usize>,
    kind: HighlightKind,
}

#[derive(Debug, Clone)]
struct PalmHighlighter {
    current_line: usize,
    settings: EditorHighlightSettings,
}

impl EditorHighlightSettings {
    fn from_source(source: &str, tokens: &[HighlightToken]) -> Self {
        let line_count = source.lines().count().max(1);
        let mut lines = vec![Vec::new(); line_count];
        let line_offsets = line_start_offsets(source);

        for token in tokens {
            let line_index = token.span.start.line.saturating_sub(1);
            let Some(line_start) = line_offsets.get(line_index).copied() else {
                continue;
            };
            if token.span.end.offset < token.span.start.offset {
                continue;
            }

            let start = token.span.start.offset.saturating_sub(line_start);
            let end = token.span.end.offset.saturating_sub(line_start);
            if start >= end {
                continue;
            }

            if let Some(line) = lines.get_mut(line_index) {
                line.push(LineHighlight {
                    range: start..end,
                    kind: token.kind,
                });
            }
        }

        Self { lines }
    }
}

impl iced_core::text::Highlighter for PalmHighlighter {
    type Settings = EditorHighlightSettings;
    type Highlight = HighlightKind;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, HighlightKind)>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            current_line: 0,
            settings: settings.clone(),
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.settings = new_settings.clone();
        self.current_line = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.current_line = line;
    }

    fn highlight_line(&mut self, _line: &str) -> Self::Iterator<'_> {
        let highlights = self
            .settings
            .lines
            .get(self.current_line)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|highlight| (highlight.range, highlight.kind))
            .collect::<Vec<_>>()
            .into_iter();
        self.current_line += 1;
        highlights
    }

    fn current_line(&self) -> usize {
        self.current_line
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedWindow {
    from_ms: i64,
    to_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CalendarMonth {
    year: i32,
    month: u32,
}

impl CalendarMonth {
    fn from_date(date: Date) -> Self {
        Self {
            year: date.year,
            month: date.month,
        }
    }

    fn shift(self, delta: i32) -> Self {
        let absolute_month = self.year * 12 + self.month as i32 - 1 + delta;
        Self {
            year: absolute_month.div_euclid(12),
            month: absolute_month.rem_euclid(12) as u32 + 1,
        }
    }

    fn label(self) -> String {
        format!("{} {}", month_name(self.month), self.year)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickedDate {
    From,
    To,
}

fn weekday_index(year: i32, month: u32, day: u32) -> u32 {
    let days = days_from_civil(year, month, day).expect("calendar date should be valid");
    (days + 4).rem_euclid(7) as u32
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
}

fn same_date(left: Date, right: Date) -> bool {
    left.year == right.year && left.month == right.month && left.day == right.day
}

#[cfg(test)]
mod tests {
    use super::{
        date_from_ms, format_date_ms, format_number, format_percent, selected_window, update,
        CalendarMonth, EditorHighlightSettings, HighlightKind, HighlightPosition, HighlightSpan,
        HighlightToken, IdeApp, Message, PublicDataset, PublicDatasetId,
    };
    use iced_aw::date_picker::Date;

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
        let window = selected_window(
            &dataset(),
            Date::from_ymd(2023, 11, 20),
            Date::from_ymd(2024, 1, 5),
        )
        .expect("window");
        assert!(window.from_ms < window.to_ms);
    }

    #[test]
    fn selected_window_rejects_out_of_bounds_range() {
        let err = selected_window(
            &dataset(),
            Date::from_ymd(2021, 1, 1),
            Date::from_ymd(2021, 1, 10),
        )
        .expect_err("range");
        assert!(err.contains("curated dataset"));
    }

    #[test]
    fn summary_formatters_handle_numbers() {
        assert_eq!(format_number(12.3456, 2), "12.35");
        assert_eq!(format_percent(18.234), "18.23%");
        assert_eq!(format_date_ms(1_700_000_000_000), "2023-11-14");
        let date = date_from_ms(1_700_000_000_000);
        assert_eq!(date.year, 2023);
        assert_eq!(date.month, 11);
        assert_eq!(date.day, 14);
    }

    #[test]
    fn picker_day_click_closes_immediately() {
        let mut app = IdeApp::default();
        let _ = update(&mut app, Message::ChooseFromDate);
        assert!(app.show_from_picker);

        let _ = update(
            &mut app,
            Message::SubmitFromDate(Date::from_ymd(2024, 1, 15)),
        );
        assert!(!app.show_from_picker);
        assert_eq!(app.from_date.year, 2024);
        assert_eq!(app.from_date.month, 1);
        assert_eq!(app.from_date.day, 15);
    }

    #[test]
    fn opening_picker_syncs_visible_month_to_selected_date() {
        let mut app = IdeApp {
            from_date: Date::from_ymd(2024, 7, 9),
            from_picker_month: CalendarMonth {
                year: 2023,
                month: 1,
            },
            ..IdeApp::default()
        };

        let _ = update(&mut app, Message::ChooseFromDate);

        assert!(app.show_from_picker);
        assert_eq!(
            app.from_picker_month,
            CalendarMonth {
                year: 2024,
                month: 7,
            }
        );
    }

    #[test]
    fn shifting_picker_month_moves_across_year_boundaries() {
        let month = CalendarMonth {
            year: 2024,
            month: 1,
        };
        assert_eq!(
            month.shift(-1),
            CalendarMonth {
                year: 2023,
                month: 12,
            }
        );
        assert_eq!(
            month.shift(13),
            CalendarMonth {
                year: 2025,
                month: 2,
            }
        );
    }

    #[test]
    fn highlight_settings_group_ranges_by_line() {
        let source = "let fast = ema(src.close, 5)\nplot(fast)";
        let settings = EditorHighlightSettings::from_source(
            source,
            &[
                HighlightToken {
                    span: HighlightSpan {
                        start: HighlightPosition { offset: 0, line: 1 },
                        end: HighlightPosition { offset: 3, line: 1 },
                    },
                    kind: HighlightKind::Keyword,
                },
                HighlightToken {
                    span: HighlightSpan {
                        start: HighlightPosition { offset: 4, line: 1 },
                        end: HighlightPosition { offset: 8, line: 1 },
                    },
                    kind: HighlightKind::Variable,
                },
                HighlightToken {
                    span: HighlightSpan {
                        start: HighlightPosition {
                            offset: 29,
                            line: 2,
                        },
                        end: HighlightPosition {
                            offset: 33,
                            line: 2,
                        },
                    },
                    kind: HighlightKind::Function,
                },
            ],
        );

        assert_eq!(settings.lines.len(), 2);
        assert_eq!(settings.lines[0][0].range, 0..3);
        assert_eq!(settings.lines[0][1].range, 4..8);
        assert_eq!(settings.lines[1][0].range, 0..4);
    }

    #[test]
    fn checked_in_bundle_avoids_direct_clipboard_method_imports() {
        let bundle = include_str!("../dist/palmscript_ide.js");
        assert!(!bundle.contains("__wbg_writeText_"));
        assert!(!bundle.contains("__wbg_readText_"));
    }

    #[test]
    fn checked_in_shell_primes_clipboard_on_load() {
        let shell = include_str!("../index.html");
        assert!(shell.contains("primeClipboardAccess"));
        assert!(shell.contains("navigator.clipboard?.readText"));
        assert!(shell.contains("palmscript-ide-clipboard-preflight"));
        assert!(shell.contains("ide-brand-slot"));
        assert!(shell.contains("object-fit: contain"));
        assert!(shell.contains("justify-content: flex-start"));
        assert!(shell.contains("object-position: center center"));
        assert!(shell.contains("max-width: 112px"));
    }
}
