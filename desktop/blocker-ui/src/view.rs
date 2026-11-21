//! The status window, built with Rune and rune-kit.

use std::sync::mpsc::Sender;

use blocker_core::status::{ExtensionState, SystemStatus};
use rune::{col, el, row, text, Color, Cx, El, ScrollSpec};
use rune_kit::{Badge, BadgeVariant, Button, ButtonVariant, Kit, PaletteSeed, Switch};

use crate::model::{ago, Activity, RuleRow, Snapshot};

/// Something the window asks the worker to do with admin rights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    SetRuleEnabled { rule: String, enabled: bool },
    UpdateLists,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub ok: bool,
    pub text: String,
}

pub struct Ui {
    pub snap: Snapshot,
    pub system: Option<SystemStatus>,
    /// Unix seconds, for "checked 5 min ago".
    pub now: u64,
    /// What the worker is doing, e.g. "Updating lists…"; disables actions.
    pub busy: Option<String>,
    pub notice: Option<Notice>,
    /// Requests made from the window, newest last (kept for tests and logs).
    pub sent: Vec<Request>,
    sink: Option<Sender<Request>>,
}

impl Ui {
    pub fn new(sink: Option<Sender<Request>>) -> Self {
        Self {
            snap: Snapshot::default(),
            system: None,
            now: 0,
            busy: None,
            notice: None,
            sent: Vec::new(),
            sink,
        }
    }

    pub fn activity(&self) -> Activity {
        self.snap.activity(self.system.as_ref())
    }

    pub fn send(&mut self, request: Request) {
        if self.busy.is_some() {
            return;
        }
        if let Some(sink) = &self.sink {
            let _ = sink.send(request.clone());
        }
        self.sent.push(request);
        if self.sent.len() > 32 {
            self.sent.remove(0);
        }
    }
}

fn kit(dark: bool) -> Kit {
    let mut k = Kit::from_palette(PaletteSeed {
        bg: if dark {
            rune::oklch(0.19, 0.005, 260.0)
        } else {
            rune::oklch(0.985, 0.002, 260.0)
        },
        fg: if dark {
            rune::oklch(0.95, 0.0, 0.0)
        } else {
            rune::oklch(0.22, 0.01, 260.0)
        },
        accent: rune::oklch(0.62, 0.17, 255.0),
    });
    // The derived knob is dark on a light background; switches read better white.
    k.c.knob = k.c.white;
    k.c.knob_on = k.c.white;
    k
}

pub fn view(s: &Ui, cx: &mut Cx<Ui>) -> El {
    let k = kit(cx.system_dark());
    let body = col()
        .key("content")
        .w_full()
        .gap(18.0)
        .px(20.0)
        .py(20.0)
        .child(header(s, &k))
        .children(s.notice.as_ref().map(|n| notice(n, &k)))
        .child(section("Rules", rules(s, &k), &k))
        .child(section("Blocklists", lists(s, &k), &k))
        .child(section("System", system(s, &k), &k));
    col()
        .key("window")
        .w_full()
        .h_full()
        .fill(k.c.bg_base)
        .child(
            col()
                .key("scroll")
                .w_full()
                .grow(1.0)
                .shrink()
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(body),
        )
}

fn activity_color(a: &Activity, k: &Kit) -> Color {
    match a {
        Activity::Blocking { .. } => k.c.success.icon,
        Activity::Idle => k.c.icon_faint,
        Activity::Attention(_) => k.c.warning.icon,
    }
}

fn header(s: &Ui, k: &Kit) -> El {
    let activity = s.activity();
    let color = activity_color(&activity, k);
    let (title, detail) = match &activity {
        Activity::Blocking { rules } => ("Blocking".to_string(), rules.join(", ")),
        Activity::Idle => (
            "Not blocking".to_string(),
            "No rule is active right now".to_string(),
        ),
        Activity::Attention(why) => ("Needs attention".to_string(), why.clone()),
    };
    row()
        .key("header")
        .w_full()
        .gap(14.0)
        .items_center()
        .child(
            el().key("status-dot")
                .size(14.0, 14.0)
                .radius(7.0)
                .fill(color)
                .role("image")
                .label(activity.headline()),
        )
        .child(
            col()
                .grow(1.0)
                .shrink()
                .gap(2.0)
                .child(
                    text(title)
                        .key("headline")
                        .font(rune::TextStyle::new(20.0, 650.0))
                        .color(k.c.text_base),
                )
                .child(
                    text(detail)
                        .key("headline-detail")
                        .font(k.ty.small)
                        .color(k.c.text_muted)
                        .truncate(),
                ),
        )
        .children(s.busy.as_ref().map(|b| {
            text(b.clone())
                .key("busy")
                .font(k.ty.small)
                .color(k.c.text_muted)
        }))
}

fn notice(n: &Notice, k: &Kit) -> El {
    let state = if n.ok { &k.c.success } else { &k.c.danger };
    row()
        .key("notice")
        .w_full()
        .px(12.0)
        .py(8.0)
        .radius(k.radius.md)
        .fill(state.bg)
        .border(1.0, state.border)
        .child(text(n.text.clone()).font(k.ty.small).color(state.fg).wrap())
}

fn section(title: &'static str, content: El, k: &Kit) -> El {
    col()
        .key(title)
        .w_full()
        .gap(8.0)
        .child(
            text(title.to_uppercase())
                .font(rune::tok::type_::SECTION)
                .color(k.c.text_faint),
        )
        .child(
            col()
                .w_full()
                .radius(k.radius.md)
                .fill(k.c.surface_raised_base)
                .border(1.0, k.c.border_muted)
                .child(content),
        )
}

fn line(key: impl Into<String>) -> El {
    row()
        .key(key.into())
        .w_full()
        .gap(10.0)
        .px(14.0)
        .py(10.0)
        .items_center()
}

/// Rows separated by hairlines, none above the first or below the last, so
/// the card's own rounded border stays clean.
fn rows(items: impl IntoIterator<Item = El>, k: &Kit) -> El {
    let mut out = col().w_full();
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            out = out.child(el().w_full().h(1.0).fill(k.c.border_muted).a11y_hidden());
        }
        out = out.child(item);
    }
    out
}

fn rules(s: &Ui, k: &Kit) -> El {
    if s.snap.rules.is_empty() {
        return line("no-rules").child(
            text("No rules yet. Add one with `sudo blocker rule add`.")
                .font(k.ty.small)
                .color(k.c.text_muted),
        );
    }
    rows(s.snap.rules.iter().map(|r| rule_row(s, r, k)), k)
}

fn rule_row(s: &Ui, r: &RuleRow, k: &Kit) -> El {
    let mut badges = row().gap(6.0);
    if r.active {
        badges = badges.child(Badge::new("Active").variant(BadgeVariant::Accent).build(k));
    }
    if r.locked {
        badges = badges.child(Badge::new("Locked").variant(BadgeVariant::Subtle).build(k));
    }
    let name = r.name.clone();
    let enabled = r.enabled;
    let toggle = Switch::new(r.enabled)
        .disabled(!r.can_toggle() || s.busy.is_some())
        .on_toggle(move |s: &mut Ui| {
            s.send(Request::SetRuleEnabled {
                rule: name.clone(),
                enabled: !enabled,
            })
        })
        .build(k)
        .key(format!("toggle-{}", r.name))
        .label(format!("Enable {}", r.name));
    line(format!("rule-{}", r.name))
        .child(
            col()
                .grow(1.0)
                .shrink()
                .gap(3.0)
                .child(
                    row()
                        .gap(8.0)
                        .items_center()
                        .child(
                            text(r.name.clone())
                                .font(k.ty.body_medium)
                                .color(if r.enabled {
                                    k.c.text_base
                                } else {
                                    k.c.text_faint
                                }),
                        )
                        .child(badges),
                )
                .child(
                    text(r.summary())
                        .font(k.ty.small)
                        .color(k.c.text_muted)
                        .truncate(),
                )
                .child(
                    text(r.schedule.clone())
                        .font(k.ty.small)
                        .color(k.c.text_faint)
                        .truncate(),
                ),
        )
        .child(toggle)
}

fn lists(s: &Ui, k: &Kit) -> El {
    let server = match &s.snap.server {
        Some(url) => format!(
            "{url} · serial {} · checked {}",
            s.snap.serial,
            ago(s.snap.checked_at, s.now)
        ),
        None => "No list server configured (`sudo blocker server <url> --public-key <key>`)".into(),
    };
    let update = Button::new("Update now")
        .variant(ButtonVariant::Outline)
        .disabled(s.snap.server.is_none() || s.busy.is_some())
        .on_press(|s: &mut Ui| s.send(Request::UpdateLists))
        .build(k)
        .key("update-lists");
    let mut items = vec![line("server")
        .child(
            text(server)
                .key("server-line")
                .font(k.ty.small)
                .color(k.c.text_muted)
                .grow(1.0)
                .shrink()
                .truncate(),
        )
        .child(update)];
    items.extend(s.snap.categories.iter().map(|c| {
        let count = match c.domains {
            Some(n) => format!("{} domains", thousands(n)),
            None => "not downloaded".into(),
        };
        line(format!("category-{}", c.name))
            .child(
                text(c.name.clone())
                    .font(k.ty.body)
                    .color(k.c.text_base)
                    .grow(1.0),
            )
            .child(text(count).font(k.ty.small).color(if c.domains.is_some() {
                k.c.text_muted
            } else {
                k.c.warning.fg
            }))
    }));
    items.extend(s.snap.warnings.iter().take(4).enumerate().map(|(i, w)| {
        line(format!("warning-{i}")).child(
            text(w.clone())
                .font(k.ty.small)
                .color(k.c.warning.fg)
                .wrap(),
        )
    }));
    rows(items, k)
}

fn system(s: &Ui, k: &Kit) -> El {
    let (daemon, daemon_ok) = match &s.system {
        None => ("checking…".to_string(), true),
        Some(sys) if sys.daemon.running => (
            format!(
                "running{}",
                sys.daemon
                    .pid
                    .map(|p| format!(" (pid {p})"))
                    .unwrap_or_default()
            ),
            true,
        ),
        Some(sys) if sys.daemon.installed => ("loaded, not running".into(), false),
        Some(_) => ("not installed (`sudo blocker install`)".into(), false),
    };
    let (filter, filter_ok) = match &s.system {
        None => ("checking…".to_string(), true),
        Some(sys) => match (sys.filter.extension, sys.filter.enabled) {
            (ExtensionState::Installed, Some(true)) => (
                format!(
                    "on{}",
                    sys.filter
                        .version
                        .as_ref()
                        .map(|v| format!(" (v{v})"))
                        .unwrap_or_default()
                ),
                true,
            ),
            (ExtensionState::Installed, _) => ("installed but turned off".into(), false),
            (ExtensionState::AwaitingApproval, _) => {
                ("waiting for approval in System Settings".into(), false)
            }
            (ExtensionState::NotInstalled, _) => {
                ("not installed (`blocker activate`)".into(), false)
            }
            (ExtensionState::Unknown, _) => (
                sys.filter.error.clone().unwrap_or_else(|| "unknown".into()),
                false,
            ),
        },
    };
    let status = |key: &str, label: &str, value: String, ok: bool| {
        line(key)
            .child(
                text(label.to_string())
                    .font(k.ty.body)
                    .color(k.c.text_base)
                    .w(110.0),
            )
            .child(
                text(value)
                    .key(format!("{key}-value"))
                    .font(k.ty.small)
                    .color(if ok { k.c.text_muted } else { k.c.warning.fg })
                    .grow(1.0)
                    .shrink()
                    .wrap(),
            )
    };
    rows(
        [
            status("filter", "Content filter", filter, filter_ok),
            status("daemon", "Daemon", daemon, daemon_ok),
            status(
                "vpn",
                "VPNs",
                "Tailscale, Twingate and private/CGNAT addresses are never filtered".into(),
                true,
            ),
        ],
        k,
    )
}

/// 1234567 -> "1,234,567"
pub fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Demo states for `--scene` snapshots and tests.
pub fn demo(s: &mut Ui, which: &str) {
    use crate::model::CategoryRow;
    use blocker_core::status::{DaemonInfo, FilterInfo};
    let rule = |name: &str, active: bool, locked: bool, enabled: bool| RuleRow {
        name: name.into(),
        enabled,
        locked,
        active,
        schedule: "mon-fri 09:00-17:00".into(),
        domains: 2,
        categories: vec!["social".into(), "ads".into()],
        apps: 1,
    };
    let active = which != "idle";
    s.now = 1_800_000_000;
    s.snap = Snapshot {
        rules: vec![
            rule("focus", active, true, true),
            rule("games", active, false, true),
            rule("evenings", false, false, false),
        ],
        server: Some("https://lists.example.com".into()),
        serial: 1_794_801_177,
        checked_at: s.now - 300,
        categories: vec![
            CategoryRow {
                name: "ads".into(),
                domains: Some(1_000_000),
            },
            CategoryRow {
                name: "social".into(),
                domains: Some(3),
            },
        ],
        warnings: vec![],
        error: None,
    };
    s.system = Some(SystemStatus {
        daemon: DaemonInfo {
            installed: true,
            running: true,
            pid: Some(412),
        },
        filter: FilterInfo {
            extension: ExtensionState::Installed,
            version: Some("0.1.0".into()),
            configured: Some(true),
            enabled: Some(which != "attention"),
            error: None,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn runtime(scene: &str) -> Runtime<Ui> {
        let mut ui = Ui::new(None);
        demo(&mut ui, scene);
        let env = Env {
            system_dark: true,
            scale: 2.0,
        };
        let mut rt = Runtime::new(ui, view, (460.0, 640.0), env);
        rt.frame(0.0);
        rt
    }

    fn text_of(rt: &Runtime<Ui>, key: &str) -> String {
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some(key))
            .and_then(|e| e.text)
            .unwrap_or_else(|| panic!("no text at {key}"))
    }

    #[test]
    fn header_shows_the_activity() {
        assert_eq!(text_of(&runtime("blocking"), "headline"), "Blocking");
        assert_eq!(
            text_of(&runtime("blocking"), "headline-detail"),
            "focus, games"
        );
        assert_eq!(text_of(&runtime("idle"), "headline"), "Not blocking");
        let attention = runtime("attention");
        assert_eq!(text_of(&attention, "headline"), "Needs attention");
        assert_eq!(text_of(&attention, "headline-detail"), "filter turned off");
        assert_eq!(
            text_of(&attention, "filter-value"),
            "installed but turned off"
        );
    }

    #[test]
    fn toggling_an_unlocked_rule_requests_the_change() {
        let mut rt = runtime("blocking");
        let (x, y) = rt.find("toggle-games").expect("games switch");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(
            rt.state.sent,
            vec![Request::SetRuleEnabled {
                rule: "games".into(),
                enabled: false
            }]
        );
    }

    #[test]
    fn a_locked_active_rule_ignores_its_switch() {
        let mut rt = runtime("blocking");
        let (x, y) = rt.find("toggle-focus").expect("focus switch");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert!(rt.state.sent.is_empty());
    }

    #[test]
    fn update_button_requests_lists_and_busy_blocks_repeats() {
        let mut rt = runtime("idle");
        let (x, y) = rt.find("update-lists").expect("update button");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.sent, vec![Request::UpdateLists]);
        rt.state.busy = Some("Updating lists…".into());
        rt.state.send(Request::UpdateLists);
        assert_eq!(rt.state.sent.len(), 1);
    }

    #[test]
    fn categories_show_formatted_counts() {
        let rt = runtime("idle");
        assert!(rt.find("category-ads").is_some());
        assert_eq!(thousands(1_000_000), "1,000,000");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(0), "0");
    }
}
