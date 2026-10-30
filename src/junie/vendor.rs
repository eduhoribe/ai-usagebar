use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::format::{placeholders, substitute, updated_at_hm};
use crate::pango::{color_span, escape};
use crate::theme::Theme;
use crate::tooltip::{Line, render_bordered};
use crate::vendor::{RenderOpts, VendorId, VendorOutcome};
use crate::waybar::{Class, WaybarOutput};

use super::types::Snapshot;

impl From<super::fetch::FetchOutcome> for VendorOutcome {
    fn from(outcome: super::fetch::FetchOutcome) -> Self {
        outcome.map(crate::usage::VendorSnapshot::Junie)
    }
}

pub fn build_placeholders(s: &Snapshot) -> HashMap<&'static str, String> {
    placeholders(vec![
        ("icon", VendorId::Junie.bar_icon().into()),
        ("vendor_short", VendorId::Junie.short_name().into()),
        ("session_pct", "".into()),
        ("session_reset", "—".into()),
        ("weekly_pct", "".into()),
        ("weekly_reset", "—".into()),
        ("plan", s.plan.clone()),
        ("junie_balance", s.balance_label()),
    ])
}

pub fn render(
    outcome: &VendorOutcome,
    s: &Snapshot,
    theme: &Theme,
    opts: &RenderOpts,
    now: DateTime<Utc>,
) -> WaybarOutput {
    let values = build_placeholders(s);
    let text = substitute(opts.format.as_deref().unwrap_or("{junie_balance}"), &values);
    let icon = opts
        .icon
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!("{s} "))
        .unwrap_or_default();
    let tooltip = if let Some(custom) = &opts.tooltip_format {
        substitute(custom, &values)
    } else {
        render_bordered(
            &[
                Line::Center(escape(VendorId::Junie.display_name())),
                Line::Sep,
                Line::Body(format!("Plan: {}", escape(&s.plan))),
                Line::Body(format!("Balance: {}", escape(&s.balance_label()))),
                Line::Body(format!("Updated {}", updated_at_hm(now, outcome.cache_age))),
            ],
            theme,
        )
    };
    WaybarOutput {
        text: color_span(&theme.fg, &format!("{icon}{text}")),
        tooltip,
        class: Class::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credit_balance_has_no_invented_percentage_or_reset() {
        let s = Snapshot {
            balance: 12.5,
            plan: "JetBrains AI Pro".into(),
        };
        let values = build_placeholders(&s);
        assert_eq!(values["junie_balance"], "12.50 credits");
        assert_eq!(values["session_pct"], "");
        assert_eq!(values["weekly_reset"], "—");
    }
}
