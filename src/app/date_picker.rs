//! 日期欄位右側的日曆 icon 與日曆選擇（民國年顯示，選完回填 YYY/MM/DD）

use super::*;
use chrono::{Datelike, NaiveDate};

const CAL_WEEK: [&str; 7] = ["日", "一", "二", "三", "四", "五", "六"];

impl BatchSchedulerApp {
    fn cal_input(&self, t: CalTarget) -> Option<&Entity<InputState>> {
        match t {
            CalTarget::RangeStart => Some(&self.exec_range_start_input),
            CalTarget::RangeEnd => Some(&self.exec_range_end_input),
            CalTarget::Specific(i) => self.exec_specific_inputs.get(i),
        }
    }

    /// 填入選到的日期並關閉日曆
    fn pick_calendar_date(
        &mut self,
        t: CalTarget,
        date: NaiveDate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let txt = RocDateUtils::to_roc_yyymmdd(&date, "/");
        if let Some(st) = self.cal_input(t).cloned() {
            st.update(cx, |s, cx| s.set_value(txt, window, cx));
        }
        self.exec_cal_open = None;
        cx.notify();
    }

    /// 清除欄位內容並關閉日曆
    fn clear_calendar_date(&mut self, t: CalTarget, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(st) = self.cal_input(t).cloned() {
            st.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.exec_cal_open = None;
        cx.notify();
    }

    fn cal_shift_month(&mut self, delta: i32, cx: &mut Context<Self>) {
        let (y, m) = self.exec_cal_ym;
        self.exec_cal_ym = RocDateUtils::shift_months(y, m, delta);
        cx.notify();
    }

    /// 日期輸入框 + 右側日曆 icon；點 icon 展開日曆，點日期即回填
    pub(super) fn render_date_field(
        &self,
        t: CalTarget,
        st: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let key = format!("{t:?}");
        let open = self.exec_cal_open == Some(t);

        let trigger = div()
            .id(SharedString::from(format!("cal-btn-{key}")))
            .w(px(24.))
            .h(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(if open { SECONDARY } else { CARD })
            .hover(|s| s.bg(SECONDARY))
            .cursor_pointer()
            .text_size(px(14.))
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    // 這次點擊已先被日曆的「點外部關閉」處理過 → 視為關閉，不再重開
                    if this.exec_cal_closed_by == Some((t, ev.position)) {
                        this.exec_cal_closed_by = None;
                        cx.notify();
                        return;
                    }
                    this.exec_cal_closed_by = None;
                    if this.exec_cal_open == Some(t) {
                        this.exec_cal_open = None;
                    } else {
                        // 以欄位目前的日期當起始月份；沒填或格式不對則用今天
                        let base = this
                            .cal_input(t)
                            .and_then(|s| RocDateUtils::parse_roc_date(s.read(cx).value().trim()))
                            .unwrap_or_else(|| Local::now().date_naive());
                        this.exec_cal_ym = (base.year(), base.month());
                        this.exec_cal_open = Some(t);
                    }
                    cx.notify();
                }),
            )
            .child("📅");

        let mut cell = div().relative().child(trigger);
        if open {
            let (y, m) = self.exec_cal_ym;
            let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap_or_else(|| Local::now().date_naive());
            let lead = first.weekday().num_days_from_sunday() as usize;
            let dim = RocDateUtils::days_in_month(y, m);
            let today = Local::now().date_naive();
            let selected = self
                .cal_input(t)
                .and_then(|s| RocDateUtils::parse_roc_date(s.read(cx).value().trim()));

            let nav_btn = |id: String, label: &'static str, delta: i32, cx: &mut Context<Self>| {
                div()
                    .id(SharedString::from(id))
                    .w(px(28.))
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .text_size(px(14.))
                    .hover(|s| s.bg(SECONDARY))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.cal_shift_month(delta, cx);
                    }))
                    .child(label)
            };

            let header = h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .child(
                    h_flex()
                        .child(nav_btn(format!("cal-py-{key}"), "«", -12, cx))
                        .child(nav_btn(format!("cal-pm-{key}"), "‹", -1, cx)),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("民國 {} 年 {} 月", y - 1911, m)),
                )
                .child(
                    h_flex()
                        .child(nav_btn(format!("cal-nm-{key}"), "›", 1, cx))
                        .child(nav_btn(format!("cal-ny-{key}"), "»", 12, cx)),
                );

            let mut week_row = h_flex().w_full();
            for w in CAL_WEEK {
                week_row = week_row.child(
                    div()
                        .w(px(32.))
                        .flex()
                        .justify_center()
                        .text_size(px(12.))
                        .text_color(MUTED_FOREGROUND)
                        .child(w),
                );
            }

            // 日期格：前面補空白到該月 1 號的星期，再補滿最後一列
            let mut cells: Vec<Option<u32>> = vec![None; lead];
            cells.extend((1..=dim).map(Some));
            while cells.len() % 7 != 0 {
                cells.push(None);
            }
            let mut grid = v_flex().w_full().gap_0p5();
            for (r, week) in cells.chunks(7).enumerate() {
                let mut row = h_flex().w_full();
                for (c, d) in week.iter().enumerate() {
                    match d {
                        None => {
                            row = row.child(div().w(px(32.)).h(px(28.)));
                        }
                        Some(d) => {
                            let d = *d;
                            let date = NaiveDate::from_ymd_opt(y, m, d);
                            let is_sel = date.is_some() && date == selected;
                            let is_today = date == Some(today);
                            row = row.child(
                                div()
                                    .id(SharedString::from(format!("cal-d-{key}-{r}-{c}")))
                                    .w(px(32.))
                                    .h(px(28.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .text_size(px(13.))
                                    .cursor_pointer()
                                    .border_1()
                                    .border_color(if is_today { PRIMARY } else { CARD })
                                    .bg(if is_sel { PRIMARY } else { CARD })
                                    .text_color(if is_sel {
                                        hsl(0.0, 0.0, 100.0)
                                    } else {
                                        FOREGROUND
                                    })
                                    .hover(|s| s.bg(SECONDARY))
                                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                        if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) {
                                            this.pick_calendar_date(t, date, window, cx);
                                        }
                                    }))
                                    .child(d.to_string()),
                            );
                        }
                    }
                }
                grid = grid.child(row);
            }

            let footer = h_flex()
                .w_full()
                .justify_between()
                .child(
                    div()
                        .id(SharedString::from(format!("cal-today-{key}")))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_size(px(13.))
                        .text_color(PRIMARY)
                        .cursor_pointer()
                        .hover(|s| s.bg(SECONDARY))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.pick_calendar_date(t, Local::now().date_naive(), window, cx);
                        }))
                        .child("今天"),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("cal-clear-{key}")))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_size(px(13.))
                        .text_color(MUTED_FOREGROUND)
                        .cursor_pointer()
                        .hover(|s| s.bg(SECONDARY))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.clear_calendar_date(t, window, cx);
                        }))
                        .child("清除"),
                );

            let popup = v_flex()
                .id(SharedString::from(format!("cal-pop-{key}")))
                .w(px(240.))
                .p_2()
                .gap_1()
                .bg(CARD)
                .border_1()
                .border_color(BORDER)
                .rounded_md()
                .shadow_lg()
                .occlude()
                .on_mouse_down_out(cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    if this.exec_cal_open == Some(t) {
                        this.exec_cal_open = None;
                        this.exec_cal_closed_by = Some((t, ev.position));
                        cx.notify();
                    }
                }))
                .child(header)
                .child(week_row)
                .child(grid)
                .child(footer);
            // 用 anchored 讓日曆自動貼齊視窗邊界：下方空間不夠時會往上移，不會被視窗截掉
            let anchored_popup = gpui::anchored()
                .anchor(gpui::Corner::TopRight)
                .offset(gpui::point(px(24.), px(28.)))
                .snap_to_window_with_margin(px(8.))
                .child(popup);
            cell = cell.child(gpui::deferred(anchored_popup).with_priority(10));
        }

        // 日曆 icon 疊在輸入框裡面、靠右
        div()
            .relative()
            .w(px(150.))
            .text_size(px(14.))
            .child(Input::new(st))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right(px(6.))
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .child(cell),
            )
            .into_any_element()
    }
}
