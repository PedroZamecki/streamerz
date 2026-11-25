//! Virtualized rows with shared action definitions for buttons and context menus.

use crate::i18n::tr;
use gpui_kit::component::{
    menu::{ContextMenuExt, PopupMenu, PopupMenuItem},
    switch::Switch,
    table::{Column, ColumnSort, TableDelegate, TableState},
    ActiveTheme, Disableable, Icon, Sizable,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value;
use std::rc::Rc;

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Add,
    Edit,
    Toggle,
    Delete,
    Connect,
    ConnectAll,
    DisconnectAll,
    Refresh,
    /// An operation the core describes itself: an index into the model's `actions`.
    Custom(usize),
}
impl Action {
    pub const ALL: [Self; 4] = [Self::Add, Self::Edit, Self::Toggle, Self::Delete];
    pub const CONNECTIONS: [Self; 4] = [Self::Connect, Self::ConnectAll, Self::DisconnectAll, Self::Refresh];
    fn named(id: &str) -> Option<Self> {
        Some(match id {
            "add" => Self::Add,
            "edit" => Self::Edit,
            "toggle" => Self::Toggle,
            "delete" => Self::Delete,
            "connect" => Self::Connect,
            "connect-all" => Self::ConnectAll,
            "disconnect-all" => Self::DisconnectAll,
            "refresh" => Self::Refresh,
            _ => return None,
        })
    }
    /// The buttons the model lists, in its order.
    pub fn base_actions(model: &Value) -> Vec<Self> {
        match model["actions"].as_array() {
            None => Self::ALL.to_vec(),
            Some(actions) => actions
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| match entry.as_str() {
                    Some(id) => Self::named(id),
                    None => entry.is_object().then_some(Self::Custom(index)),
                })
                .collect(),
        }
    }
    pub fn actions(model: &Value) -> Vec<Self> {
        let mut actions = Self::base_actions(model);
        if model["switchToggle"] == true && !actions.iter().any(|action| matches!(action, Self::Toggle)) {
            actions.push(Self::Toggle);
        }
        if model["connection"].is_object() {
            if !actions.iter().any(|action| matches!(action, Self::Connect)) {
                actions.push(Self::Connect);
            }
            if model["rows"].as_array().is_some_and(|rows| rows.len() > 1) {
                actions.push(Self::ConnectAll);
                actions.push(Self::DisconnectAll);
            }
        }
        actions
    }
    /// A stable id for the core's `can` flags and for button ids.
    pub fn id(self, model: &Value) -> String {
        match self {
            Self::Add => "add".into(),
            Self::Edit => "edit".into(),
            Self::Toggle => "toggle".into(),
            Self::Delete => "delete".into(),
            Self::Connect => "connect".into(),
            Self::ConnectAll => "connect-all".into(),
            Self::DisconnectAll => "disconnect-all".into(),
            Self::Refresh => "refresh".into(),
            Self::Custom(index) => model["actions"][index]["id"].as_str().unwrap_or_default().to_owned(),
        }
    }
    pub fn label(self, model: &Value) -> String {
        match self {
            Self::Custom(index) => model["actions"][index]["label"].as_str().unwrap_or_default().to_owned(),
            _ => tr(match self {
                Self::Add => "btn.add",
                Self::Edit => "btn.edit",
                Self::Toggle => "btn.on_off",
                Self::Delete => "btn.delete",
                Self::Connect => "btn.connect",
                Self::ConnectAll => "btn.connect_all",
                Self::DisconnectAll => "btn.disconnect_all",
                _ => "btn.refresh",
            })
            .to_owned(),
        }
    }
    pub fn icon(self, model: &Value) -> Icon {
        match self {
            Self::Custom(index) => Icon::empty().path(format!(
                "icons/{}.svg",
                model["actions"][index]["icon"].as_str().unwrap_or("circle")
            )),
            _ => Icon::empty().path(match self {
                Self::Add => "icons/plus.svg",
                Self::Edit => "icons/pencil.svg",
                Self::Toggle => "icons/power.svg",
                Self::Delete => "icons/trash-2.svg",
                Self::Connect | Self::ConnectAll => "icons/plug.svg",
                Self::DisconnectAll => "icons/unplug.svg",
                _ => "icons/refresh-cw.svg",
            }),
        }
    }
    pub fn enabled(self, model: &Value, row: Option<usize>) -> bool {
        let row = row.filter(|row| *row < model["rows"].as_array().map_or(0, Vec::len));
        let allowed = |row: usize, id: &str| model["rows"][row]["can"][id] != false;
        match self {
            Self::Connect if row.is_none() && model["connection"].is_object() => {
                model["rows"].as_array().is_some_and(|rows| !rows.is_empty())
            }
            Self::ConnectAll => model["connection"]["canConnect"] == true,
            Self::DisconnectAll => model["connection"]["canDisconnect"] == true,
            Self::Refresh | Self::Add => true,
            Self::Custom(index) => {
                let spec = &model["actions"][index];
                spec["enabled"] != false
                    && (spec["needs"] != "row"
                        || row.is_some_and(|row| allowed(row, spec["id"].as_str().unwrap_or_default())))
            }
            _ => row.is_some_and(|row| allowed(row, &self.id(model))),
        }
    }
    /// Why the button is off, when the core says (shown on hover).
    pub fn tip(self, model: &Value) -> Option<String> {
        match self {
            Self::Custom(index) => model["actions"][index]["tip"].as_str().map(str::to_owned),
            _ => None,
        }
    }
    pub fn row_label(self, model: &Value, row: Option<usize>) -> String {
        if matches!(self, Self::Connect) {
            match row {
                Some(row) if model["rows"][row]["connected"] == true => return tr("btn.disconnect").to_owned(),
                Some(_) => return tr("btn.connect").to_owned(),
                None if model["connection"].is_object() => {
                    let count = model["rows"].as_array().map_or(0, Vec::len);
                    if count == 0 {
                        return tr("btn.connect").to_owned();
                    }
                    if model["connection"]["canConnect"] == true {
                        return tr(if count > 1 { "btn.connect_all" } else { "btn.connect" }).to_owned();
                    }
                    return tr(if count > 1 {
                        "btn.disconnect_all"
                    } else {
                        "btn.disconnect"
                    })
                    .to_owned();
                }
                _ => {}
            }
        }
        self.label(model)
    }
    pub fn row_icon(self, model: &Value, row: Option<usize>) -> Icon {
        if matches!(self, Self::Connect) {
            let disconnecting = match row {
                Some(row) => model["rows"][row]["connected"] == true,
                None => {
                    model["rows"].as_array().is_some_and(|rows| !rows.is_empty())
                        && model["connection"]["canConnect"] != true
                }
            };
            return Icon::empty().path(if disconnecting {
                "icons/unplug.svg"
            } else {
                "icons/plug.svg"
            });
        }
        self.icon(model)
    }
}
pub type Handler = Rc<dyn Fn(Action, usize, &mut Window, &mut App)>;
/// Called with a column key and whether it should sort descending, when a header asks for it.
pub type Sorter = Rc<dyn Fn(String, bool, &mut Window, &mut App)>;
pub struct Rows {
    pub model: Value,
    pub action: Handler,
    pub sort: Sorter,
    /// The width the table has to draw in (0 until it is known): columns shrink to fit it and the
    /// last one takes what is left, so a narrow window needs no horizontal scrolling.
    pub fit: f32,
    visible: Vec<usize>,
    fitted: Vec<f32>,
}
impl Rows {
    pub fn new(model: Value, action: Handler, sort: Sorter) -> Self {
        let mut rows = Self {
            model,
            action,
            sort,
            fit: 0.,
            visible: Vec::new(),
            fitted: Vec::new(),
        };
        rows.layout();
        rows
    }

    /// Recompute only when the model or measured width changes, never per visible cell.
    pub fn layout(&mut self) {
        let columns = self.model["columns"].as_array().map_or(&[][..], Vec::as_slice);
        let rows = self.model["rows"].as_array().map_or(&[][..], Vec::as_slice);
        let mut widths = Vec::with_capacity(columns.len());
        let mut naturals = Vec::with_capacity(columns.len());
        let mut mins = Vec::with_capacity(columns.len());
        let mut priorities = Vec::with_capacity(columns.len());
        let mut hidden = Vec::with_capacity(columns.len());
        for (index, column) in columns.iter().enumerate() {
            let sample = rows.iter().take(64).map(|row| &row["cells"][index]);
            let mut longest = 0;
            let mut logos = 0;
            let mut is_switch = false;
            let mut has_icon = false;
            for value in sample {
                is_switch |= value.is_boolean();
                has_icon |= value["icon"].is_string() || value["brand"].is_string() || value["health"].is_string();
                logos = logos.max(value["platforms"].as_array().map_or(0, Vec::len));
                longest = longest.max(
                    value
                        .as_str()
                        .or_else(|| value["text"].as_str())
                        .map_or(0, |s| s.chars().count()),
                );
            }
            let key = column["key"].as_str().unwrap_or_default();
            let header_chars = column["label"].as_str().map_or(0, |s| s.chars().count());
            let essential = is_switch || matches!(key, "name" | "command" | "action" | "what" | "agent" | "when");
            let priority = column["priority"]
                .as_u64()
                .unwrap_or(if essential { 0 } else { 1 + index as u64 });
            let icon_extra = if has_icon { 22. } else { 0. };
            let header_min = if is_switch {
                48.
            } else {
                (28. + header_chars as f32 * 7.2).min(110.)
            };
            let content_min = if is_switch {
                48.
            } else if logos > 0 {
                logos as f32 * 24. + 16.
            } else if longest == 0 {
                48.
            } else {
                (24. + icon_extra + longest as f32 * 6.4).clamp(64., if essential { 128. } else { 96. })
            };
            let min = content_min.max(header_min);
            let preferred = column["width"].as_f64().unwrap_or(160.) as f32;
            let content_width = 24. + icon_extra + longest as f32 * 7.6;
            let base = if is_switch || logos > 0 {
                min
            } else {
                preferred.min(content_width.max(min)).max(min)
            };
            let natural = if is_switch || logos > 0 {
                min
            } else {
                content_width.max(preferred).max(min).min(420.)
            };
            widths.push(base);
            naturals.push(natural);
            mins.push(min);
            priorities.push(priority);
            hidden.push(column["hidden"] == true);
        }
        self.visible = visible_columns(&mins, &priorities, &hidden, self.fit);
        self.fitted = self.visible.iter().map(|&index| widths[index]).collect();
        let visible_mins: Vec<_> = self.visible.iter().map(|&index| mins[index]).collect();
        let visible_naturals: Vec<_> = self.visible.iter().map(|&index| naturals[index]).collect();
        // Expand columns below their natural content width first, then share leftover room across flexible text columns.
        let total: f32 = self.fitted.iter().sum();
        if total < self.fit {
            let mut spare = self.fit - total;
            let gaps: Vec<f32> = self
                .fitted
                .iter()
                .zip(visible_naturals.iter())
                .map(|(w, nat)| (nat - w).max(0.))
                .collect();
            let gap_sum: f32 = gaps.iter().sum();
            if gap_sum > 0. {
                let take = spare.min(gap_sum);
                for (w, g) in self.fitted.iter_mut().zip(gaps.iter()) {
                    *w += take * (*g / gap_sum);
                }
                spare -= take;
            }
            if spare > 0.5 {
                let flex: Vec<usize> = self
                    .visible
                    .iter()
                    .enumerate()
                    .filter(|(_, index)| {
                        matches!(
                            columns[**index]["key"].as_str(),
                            Some(
                                "description"
                                    | "what"
                                    | "command"
                                    | "config"
                                    | "text"
                                    | "message"
                                    | "action"
                                    | "name"
                                    | "why"
                            )
                        )
                    })
                    .map(|(pos, _)| pos)
                    .collect();
                if flex.is_empty() {
                    if let Some(width) = self.fitted.last_mut() {
                        *width += spare;
                    }
                } else {
                    let per = spare / flex.len() as f32;
                    for pos in flex {
                        if let Some(width) = self.fitted.get_mut(pos) {
                            *width += per;
                        }
                    }
                }
            }
        } else {
            fit_widths_above(&mut self.fitted, &visible_mins, self.fit);
        }
    }

    pub fn hidden_columns(&self) -> impl Iterator<Item = usize> + '_ {
        let columns = self.model["columns"].as_array();
        (0..columns.map_or(0, Vec::len)).filter(move |index| {
            columns.is_some_and(|cols| cols[*index]["hidden"] != true) && !self.visible.contains(index)
        })
    }

    fn source_column(&self, index: usize) -> usize {
        self.visible.get(index).copied().unwrap_or(index)
    }
    pub fn id(&self, row: usize) -> Option<&str> {
        self.model["rows"][row]["id"].as_str()
    }
}
impl TableDelegate for Rows {
    fn columns_count(&self, _: &App) -> usize {
        self.visible.len()
    }
    fn rows_count(&self, _: &App) -> usize {
        self.model["rows"].as_array().map_or(0, Vec::len)
    }
    fn column(&self, index: usize, _: &App) -> Column {
        let column = &self.model["columns"][self.source_column(index)];
        let key = column["key"].as_str().unwrap_or_default();
        let mut built = Column::new(key.to_owned(), column["label"].as_str().unwrap_or_default().to_owned())
            .width(px(self.fitted.get(index).copied().unwrap_or(160.)))
            .resizable(false)
            .paddings(Edges {
                top: px(0.),
                right: px(crate::theme::SPACE_MD),
                bottom: px(0.),
                left: px(crate::theme::SPACE_MD),
            });
        // A sortable column shows the model's current sort; the core sorts, so it covers every row.
        if column["sortable"] == true {
            built = if self.model["sort"]["column"] != key {
                built.sortable()
            } else if self.model["sort"]["desc"] == true {
                built.descending()
            } else {
                built.ascending()
            };
        }
        built
    }
    fn render_header(&mut self, _: &mut Window, cx: &mut Context<TableState<Self>>) -> Stateful<Div> {
        // A quiet small header line above the box.
        div()
            .id("header")
            .text_size(px(crate::theme::FONT_CAPTION))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(crate::theme::quiet(cx))
    }
    fn render_th(&mut self, index: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        // A quiet small label above the box.
        // The sorted column is the bright one with an arrow; a sortable one sorts on a click.
        let column = &self.model["columns"][self.source_column(index)];
        let key = column["key"].as_str().unwrap_or_default().to_owned();
        let sorted = self.model["sort"]["column"] == key.as_str();
        let desc_first = column["descFirst"] == true;
        let flip = if sorted {
            self.model["sort"]["desc"] != true
        } else {
            desc_first
        };
        let sort = self.sort.clone();
        let click_key = key.clone();
        div()
            .id(("th", index))
            .size_full()
            .flex()
            .items_center()
            .gap_1()
            .text_size(px(crate::theme::FONT_CAPTION))
            .font_weight(if sorted { FontWeight::BOLD } else { FontWeight::SEMIBOLD })
            .when(sorted, |th| th.text_color(crate::theme::strong(cx)))
            .when(column["sortable"] == true, |th| {
                th.cursor_pointer()
                    .on_click(move |_, window, cx| sort(click_key.clone(), flip, window, cx))
            })
            // A label longer than its column ends in an ellipsis (it was cut mid-word), whole on hover.
            .min_w_0()
            .overflow_hidden()
            .tooltip({
                let name = self.column(index, cx).name.clone();
                move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name.clone()).build(window, cx)
            })
            .child(div().min_w_0().truncate().child(self.column(index, cx).name.clone()))
            .when(sorted, |th| {
                th.child(
                    gpui_kit::component::Icon::empty()
                        .path(if self.model["sort"]["desc"] == true {
                            "icons/arrow-down.svg"
                        } else {
                            "icons/arrow-up.svg"
                        })
                        .size_3(),
                )
            })
    }
    fn perform_sort(&mut self, index: usize, _: ColumnSort, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let column = &self.model["columns"][self.source_column(index)];
        let key = column["key"].as_str().unwrap_or_default().to_owned();
        // The same column flips; another starts as the model asks (dates newest first, text A to Z).
        let desc = if self.model["sort"]["column"] == key.as_str() {
            self.model["sort"]["desc"] != true
        } else {
            column["descFirst"] == true
        };
        (self.sort)(key, desc, window, cx);
    }
    fn render_td(
        &mut self,
        row: usize,
        column: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let column = self.source_column(column);
        // Every cell is centred in its row.
        let content = (|| -> AnyElement {
            let record = &self.model["rows"][row];
            let value = &record["cells"][column];
            if let Some(checked) = value.as_bool() {
                let action = self.action.clone();
                return Switch::new(("row-switch", row))
                    .small()
                    .accessibility_label(record["name"].as_str().unwrap_or_default().to_owned())
                    .checked(checked)
                    .disabled(
                        self.model["switchToggle"] != true
                            && !Action::base_actions(&self.model)
                                .iter()
                                .any(|action| matches!(action, Action::Toggle)),
                    )
                    .on_click(move |_, window, cx| action(Action::Toggle, row, window, cx))
                    .into_any_element();
            }
            if let Some(platforms) = value["platforms"].as_array() {
                return div()
                    .flex()
                    .items_center()
                    .gap(px(crate::theme::SPACE_SM))
                    .children(platforms.iter().filter_map(Value::as_str).map(|platform| {
                        img(format!("brands/{platform}.svg"))
                            .id(SharedString::from(platform.to_owned()))
                            .size_4()
                            .flex_shrink_0()
                    }))
                    .into_any_element();
            }
            let text = value
                .as_str()
                .or_else(|| value["text"].as_str())
                .unwrap_or_default()
                .to_owned();
            // A cell that no longer fits ends in an ellipsis instead of running under its neighbour.
            let words = |text: String| {
                let tip = text.clone();
                div()
                    .id(SharedString::from(format!("cell-txt-{row}-{column}")))
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(text)
                    .when(!tip.is_empty(), |el| {
                        el.tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                        })
                    })
            };
            let mut cell = div().flex().items_center().gap_2().min_w_0().overflow_hidden();
            if let Some(health) = value["health"].as_str() {
                let (icon, color) = super::health::appearance(health, cx);
                return cell
                    .text_color(color)
                    .child(Icon::empty().path(format!("icons/{icon}.svg")).small())
                    .child(words(text))
                    .into_any_element();
            }
            if value.is_object() || record["enabled"] == false {
                cell = cell.text_color(cx.theme().muted_foreground);
            }
            if !text.is_empty() || value["brand"].is_string() {
                if let Some(brand) = value["brand"].as_str() {
                    cell = cell.child(super::body::brand_logo("brand", brand, px(16.), cx));
                } else if let Some(icon) = value["icon"].as_str() {
                    cell = cell.child(Icon::empty().path(format!("icons/{icon}.svg")).small());
                } else if column == 1 && record["enabled"] == false {
                    cell = cell.child(Icon::empty().path("icons/power-off.svg").small());
                }
            }
            cell.child(words(text)).into_any_element()
        })();
        div().size_full().flex().items_center().child(content)
    }
    fn cell_label(&self, row: usize, column: usize, _: &App) -> Option<SharedString> {
        let cell = &self.model["rows"][row]["cells"][self.source_column(column)];
        cell.as_str()
            .or_else(|| cell["text"].as_str())
            .map(|text| text.to_owned().into())
    }
    fn context_menu(
        &mut self,
        row: usize,
        mut menu: PopupMenu,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        for action in Action::actions(&self.model) {
            let handler = self.action.clone();
            menu = menu.item(
                PopupMenuItem::new(action.row_label(&self.model, Some(row)))
                    .icon(action.row_icon(&self.model, Some(row)))
                    .disabled(!action.enabled(&self.model, Some(row)))
                    .on_click(move |_, window, cx| handler(action, row, window, cx)),
            );
        }
        menu
    }
    fn render_empty(&mut self, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let handler = self.action.clone();
        let model = self.model.clone();
        div()
            .id("empty-rows")
            .test_support()
            .aria_label(self.model["empty"].as_str().unwrap_or_default().to_owned())
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            // A long message wraps inside the table instead of running past both its sides.
            .child(
                div()
                    .min_w_0()
                    .max_w_full()
                    .px_4()
                    .text_center()
                    .child(self.model["empty"].as_str().unwrap_or_default().to_owned()),
            )
            .context_menu(move |menu, _, _| {
                Action::actions(&model).into_iter().fold(menu, |menu, action| {
                    let handler = handler.clone();
                    menu.item(
                        PopupMenuItem::new(action.row_label(&model, None))
                            .icon(action.row_icon(&model, None))
                            .disabled(!action.enabled(&model, None))
                            .on_click(move |_, window, cx| handler(action, usize::MAX, window, cx)),
                    )
                })
            })
    }
}

/// Keep identifying columns and remove the least important until the remaining ones are readable.
fn visible_columns(mins: &[f32], priorities: &[u64], hidden: &[bool], fit: f32) -> Vec<usize> {
    let mut visible: Vec<_> = (0..mins.len())
        .filter(|&index| !hidden.get(index).copied().unwrap_or(false))
        .collect();
    if fit <= 0. {
        return visible;
    }
    while visible.len() > 1 && visible.iter().map(|&index| mins[index]).sum::<f32>() > fit {
        let candidate = visible
            .iter()
            .enumerate()
            .filter(|(_, index)| priorities[**index] > 0)
            .max_by_key(|(_, index)| priorities[**index])
            .map(|(position, _)| position);
        let Some(position) = candidate else { break };
        visible.remove(position);
    }
    visible
}

/// The narrowest a column shrinks to.
const MIN_WIDTH: f32 = 48.;

/// Narrows `widths` to `fit` (each keeps a readable minimum), or widens the last one to fill it.
/// `fit` 0 means unknown: nothing changes.
#[cfg(test)]
pub fn fit_widths(widths: &mut [f32], fit: f32) {
    fit_widths_above(widths, &vec![MIN_WIDTH; widths.len()], fit);
}

/// [`fit_widths`], each column no narrower than its own minimum.
pub fn fit_widths_above(widths: &mut [f32], mins: &[f32], fit: f32) {
    let min = |index: usize| mins.get(index).copied().unwrap_or(MIN_WIDTH);
    for (index, width) in widths.iter_mut().enumerate() {
        *width = width.max(min(index));
    }
    let total: f32 = widths.iter().sum();
    if fit <= 0. || widths.is_empty() || total <= 0. {
        return;
    }
    if total <= fit {
        if let Some(last) = widths.last_mut() {
            *last += fit - total;
        }
        return;
    }
    // Shrink what is above the minimum in proportion until the sum fits (or all sit at the minimum).
    for _ in 0..4 {
        let total: f32 = widths.iter().sum();
        if total <= fit + 0.5 {
            break;
        }
        let spare: f32 = widths
            .iter()
            .enumerate()
            .map(|(i, width)| (width - min(i)).max(0.))
            .sum();
        if spare <= 0. {
            break;
        }
        let cut = ((total - fit) / spare).min(1.);
        for (index, width) in widths.iter_mut().enumerate() {
            *width -= (*width - min(index)).max(0.) * cut;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fit_widths, Action};
    use serde_json::json;

    fn ids(model: &serde_json::Value) -> Vec<String> {
        Action::actions(model)
            .into_iter()
            .map(|action| action.id(model))
            .collect()
    }

    #[test]
    fn a_model_lists_its_buttons_and_the_defaults_fill_in() {
        assert_eq!(ids(&json!({})), ["add", "edit", "toggle", "delete"]);
        let model = json!({"actions": ["edit", "no-such", {"id": "test", "label": "Test", "icon": "play"}, 7]});
        assert_eq!(ids(&model), ["edit", "test"]);
        assert!(matches!(Action::actions(&model)[1], Action::Custom(2)));
        assert_eq!(Action::Custom(2).label(&model), "Test");
        // A switch column adds On/Off; connections add Connect, and all of them for two rows or more.
        let model = json!({"actions": ["add"], "switchToggle": true, "connection": {}, "rows": [{}, {}]});
        assert_eq!(
            ids(&model),
            ["add", "toggle", "connect", "connect-all", "disconnect-all"]
        );
        let one = json!({"actions": [], "connection": {}, "rows": [{}]});
        assert_eq!(ids(&one), ["connect"]);
    }

    #[test]
    fn row_buttons_follow_the_cores_can_flags() {
        let model = json!({
            "actions": ["edit", "delete", {"id": "test", "needs": "row"}, {"id": "off", "enabled": false}],
            "rows": [{"can": {"delete": false}}, {"can": {"test": false}}]
        });
        assert!(Action::Edit.enabled(&model, Some(0)));
        assert!(!Action::Delete.enabled(&model, Some(0)));
        assert!(Action::Delete.enabled(&model, Some(1)));
        assert!(!Action::Edit.enabled(&model, None), "a row action needs a row");
        assert!(
            !Action::Edit.enabled(&model, Some(usize::MAX)),
            "a row past the end is none"
        );
        assert!(Action::Add.enabled(&model, None));
        assert!(Action::Custom(2).enabled(&model, Some(0)));
        assert!(!Action::Custom(2).enabled(&model, Some(1)));
        assert!(!Action::Custom(2).enabled(&model, None));
        assert!(!Action::Custom(3).enabled(&model, Some(0)));
    }

    #[test]
    fn connect_says_what_it_will_do() {
        let model = json!({"connection": {"canConnect": true}, "rows": [{"connected": true}, {"connected": false}]});
        assert_eq!(Action::Connect.row_label(&model, Some(0)), "btn.disconnect");
        assert_eq!(Action::Connect.row_label(&model, Some(1)), "btn.connect");
        assert_eq!(Action::Connect.row_label(&model, None), "btn.connect_all");
        let all_up = json!({"connection": {"canConnect": false}, "rows": [{}, {}]});
        assert_eq!(Action::Connect.row_label(&all_up, None), "btn.disconnect_all");
        let empty = json!({"connection": {}, "rows": []});
        assert_eq!(Action::Connect.row_label(&empty, None), "btn.connect");
        assert!(!Action::Connect.enabled(&empty, None));
        assert!(Action::Connect.enabled(&model, None));
    }

    #[test]
    fn narrow_tables_drop_secondary_columns_without_hiding_identity() {
        let minimums = [48., 120., 120., 96., 99., 96., 96.];
        let priorities = [0, 0, 0, 4, 5, 6, 7];
        let hidden = [false; 7];
        assert_eq!(
            super::visible_columns(&minimums, &priorities, &hidden, 400.),
            vec![0, 1, 2, 3]
        );
        assert_eq!(
            super::visible_columns(&minimums, &priorities, &hidden, 900.),
            (0..7).collect::<Vec<_>>()
        );
        assert_eq!(
            super::visible_columns(&minimums, &priorities, &hidden, 0.),
            (0..7).collect::<Vec<_>>()
        );
    }

    #[test]
    fn hidden_columns_stay_out_of_visible_tables() {
        let minimums = [120., 280., 80.];
        let priorities = [0, 2, 3];
        let hidden = [false, true, false];
        assert_eq!(
            super::visible_columns(&minimums, &priorities, &hidden, 900.),
            vec![0, 2]
        );
        assert_eq!(super::visible_columns(&minimums, &priorities, &hidden, 0.), vec![0, 2]);
    }

    #[test]
    fn a_column_of_logos_keeps_room_for_all_of_them() {
        // Commands at 540 px: Sources once shrank until Kick's logo was a green bar and TikTok's gone.
        let mut widths = [72., 200., 160., 120., 130., 100., 140.];
        let mut mins = [48.; 7];
        mins[4] = 4. * 16. + 3. * 8. + 24.;
        super::fit_widths_above(&mut widths, &mins, 420.);
        assert!(widths[4] >= mins[4] - 0.01, "{widths:?}");
        assert!(widths.iter().all(|width| *width >= 48. - 0.01));
    }

    #[test]
    fn columns_shrink_to_a_narrow_table_and_the_last_fills_a_wide_one() {
        let mut widths = [72., 200., 320., 140.];
        fit_widths(&mut widths, 400.);
        assert!((widths.iter().sum::<f32>() - 400.).abs() < 1., "{widths:?}");
        assert!(widths.iter().all(|width| *width >= 48. - 0.01));
        let mut widths = [72., 200.];
        fit_widths(&mut widths, 600.);
        assert_eq!(widths, [72., 528.]);
        let mut widths = [72., 200.];
        fit_widths(&mut widths, 0.);
        assert_eq!(widths, [72., 200.]);
        let mut widths = [100., 100., 100.];
        fit_widths(&mut widths, 90.);
        assert!(
            widths.iter().all(|width| (*width - 48.).abs() < 0.01),
            "minimum holds: {widths:?}"
        );
    }
}
