//! The tray icon that shows whether blocking is active.
//!
//! - Windows and Linux use Rune's own [`rune::Tray`]. It can't change its icon
//!   once made, so a state change replaces the tray with one showing the new
//!   icon (drawn by [`icon`]); menu and tooltip updates go to the live tray.
//! - macOS: Rune's tray doesn't support it yet and leaves the tray to the
//!   app, so it's an `NSStatusItem` driven from Rune's main thread through
//!   `AppHandle::update`. The icon is a template SF Symbol that follows the
//!   menu bar's light/dark appearance.
//!
//! Both show the same state: `shield.fill` / green dot while a rule is
//! blocking, `shield` / grey ring when nothing is active, and
//! `exclamationmark.shield.fill` / amber "!" when something needs attention.

use crate::model::{Activity, Snapshot};

/// What a menu pick asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    UpdateLists,
    Quit,
}

/// SF Symbol name and accessibility description for an activity.
pub fn symbol(activity: &Activity) -> (&'static str, &'static str) {
    match activity {
        Activity::Blocking { .. } => ("shield.fill", "Blocker: blocking"),
        Activity::Idle => ("shield", "Blocker: not blocking"),
        Activity::Attention(_) => ("exclamationmark.shield.fill", "Blocker: needs attention"),
    }
}

/// Informational lines at the top of the menu: the headline, then one line
/// per active rule with its schedule.
pub fn status_lines(activity: &Activity, snap: &Snapshot) -> Vec<String> {
    let mut lines = vec![activity.headline()];
    for rule in snap.rules.iter().filter(|r| r.active) {
        lines.push(format!(
            "{}{} — {}",
            rule.name,
            if rule.locked { " 🔒" } else { "" },
            rule.schedule
        ));
    }
    lines
}

#[cfg_attr(target_os = "macos", allow(dead_code))] // Rune's tray backend (Windows, Linux)
/// Rune tray menu: status lines (disabled), then the actions. Item ids are
/// what [`action_for`] maps back.
pub fn rune_menu(activity: &Activity, snap: &Snapshot) -> Vec<rune::TrayItem> {
    use rune::TrayItem;
    let mut menu: Vec<TrayItem> = status_lines(activity, snap)
        .into_iter()
        .enumerate()
        .map(|(i, line)| TrayItem::action(format!("status-{i}"), line).enabled(false))
        .collect();
    menu.push(TrayItem::Separator);
    menu.push(TrayItem::action("open", "Open Blocker…"));
    menu.push(TrayItem::action("update", "Update Lists Now"));
    menu.push(TrayItem::Separator);
    menu.push(TrayItem::action("quit", "Quit Blocker"));
    menu
}

#[cfg_attr(target_os = "macos", allow(dead_code))] // Rune's tray backend (Windows, Linux)
/// Maps a Rune tray event to an app action: a primary click opens the window.
pub fn action_for(event: &rune::TrayEvent) -> Option<TrayAction> {
    match event {
        rune::TrayEvent::Activate { .. } => Some(TrayAction::Open),
        rune::TrayEvent::Menu(id) => match id.as_str() {
            "open" => Some(TrayAction::Open),
            "update" => Some(TrayAction::UpdateLists),
            "quit" => Some(TrayAction::Quit),
            _ => None,
        },
        _ => None,
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))] // Rune's tray backend (Windows, Linux)
/// Side of the drawn Rune tray icon, in pixels.
pub const ICON_SIZE: u32 = 32;

#[cfg_attr(target_os = "macos", allow(dead_code))] // Rune's tray backend (Windows, Linux)
/// The Rune tray icon for an activity: a green dot while blocking, a grey
/// ring when idle, an amber dot with a white "!" when something needs
/// attention. Straight RGBA, antialiased at the edges.
pub fn icon(activity: &Activity) -> rune::TrayIcon {
    const GREEN: [u8; 3] = [52, 199, 89];
    const GREY: [u8; 3] = [142, 142, 147];
    const AMBER: [u8; 3] = [255, 159, 10];
    let n = ICON_SIZE as usize;
    let c = (n as f32 - 1.0) / 2.0;
    let r = n as f32 / 2.0 - 2.0;
    let mut px = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32 - c, y as f32 - c);
            let d = (fx * fx + fy * fy).sqrt();
            let disc = (r + 0.5 - d).clamp(0.0, 1.0);
            let (rgb, alpha) = match activity {
                Activity::Blocking { .. } => (GREEN, disc),
                Activity::Idle => {
                    let ring = (3.0 - (d - (r - 1.5)).abs()).clamp(0.0, 1.0)
                        * disc.max((r + 0.5 - d).clamp(0.0, 1.0));
                    (GREY, ring)
                }
                Activity::Attention(_) => {
                    // A white "!": a bar from 25% to 60% of the height and a dot at 72%.
                    let bar = fx.abs() <= 2.0 && fy >= -c * 0.5 && fy <= c * 0.2;
                    let dot = (fx * fx + (fy - c * 0.45).powi(2)).sqrt() <= 2.2;
                    if (bar || dot) && disc > 0.0 {
                        ([255, 255, 255], disc)
                    } else {
                        (AMBER, disc)
                    }
                }
            };
            let i = (y * n + x) * 4;
            px[i..i + 3].copy_from_slice(&rgb);
            px[i + 3] = (alpha * 255.0).round() as u8;
        }
    }
    rune::TrayIcon::rgba(ICON_SIZE, ICON_SIZE, px)
}

#[cfg(not(target_os = "macos"))]
pub use rune_tray::Tray;

#[cfg(not(target_os = "macos"))]
mod rune_tray {
    use std::sync::Arc;

    use super::{action_for, icon, rune_menu, symbol, TrayAction};
    use crate::model::{Activity, Snapshot};

    pub struct Tray {
        tray: Option<rune::Tray>,
        on_action: Arc<dyn Fn(TrayAction) + Send + Sync>,
        shown: Option<&'static str>,
        titles: Vec<String>,
    }

    impl Tray {
        pub fn new(on_action: impl Fn(TrayAction) + Send + Sync + 'static) -> Self {
            Self {
                tray: None,
                on_action: Arc::new(on_action),
                shown: None,
                titles: Vec::new(),
            }
        }

        pub fn show(&mut self, activity: &Activity, snap: &Snapshot) {
            let (name, _) = symbol(activity);
            let menu = rune_menu(activity, snap);
            self.titles = menu
                .iter()
                .map(|i| match i {
                    rune::TrayItem::Action { label, .. } => label.clone(),
                    _ => String::new(),
                })
                .collect();
            let tooltip = activity.headline();
            match &self.tray {
                Some(tray) if self.shown == Some(name) => {
                    tray.set_menu(menu);
                    tray.set_tooltip(tooltip);
                }
                _ => {
                    // Drop the old icon first so the new one takes its place.
                    self.tray = None;
                    let spec = rune::TraySpec::new("dev.blocker.ui", "Blocker", icon(activity))
                        .tooltip(tooltip)
                        .menu(menu);
                    let on_action = Arc::clone(&self.on_action);
                    match rune::Tray::new(spec, move |event| {
                        if let Some(action) = action_for(&event) {
                            on_action(action);
                        }
                    }) {
                        Ok(tray) => self.tray = Some(tray),
                        Err(e) => eprintln!("blocker-ui: no tray icon: {e}"),
                    }
                    self.shown = Some(name);
                }
            }
        }

        pub fn menu_titles(&self) -> Vec<String> {
            self.titles.clone()
        }

        pub fn symbol_shown(&self) -> Option<&'static str> {
            self.shown
        }
    }
}

#[cfg(target_os = "macos")]
pub use mac::Tray;

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
    };
    use objc2_foundation::NSString;

    use super::{status_lines, symbol, TrayAction};
    use crate::model::{Activity, Snapshot};

    struct TargetIvars {
        on_action: Box<dyn Fn(TrayAction)>,
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements; the ivars need no
        // special Drop handling and are only touched on the main thread.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "BlockerTrayTarget"]
        #[ivars = TargetIvars]
        struct Target;

        impl Target {
            #[unsafe(method(open:))]
            fn open(&self, _sender: Option<&AnyObject>) {
                (self.ivars().on_action)(TrayAction::Open);
            }

            #[unsafe(method(updateLists:))]
            fn update_lists(&self, _sender: Option<&AnyObject>) {
                (self.ivars().on_action)(TrayAction::UpdateLists);
            }

            #[unsafe(method(quit:))]
            fn quit(&self, _sender: Option<&AnyObject>) {
                (self.ivars().on_action)(TrayAction::Quit);
            }
        }
    );

    impl Target {
        fn new(mtm: MainThreadMarker, on_action: Box<dyn Fn(TrayAction)>) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(TargetIvars { on_action });
            // SAFETY: NSObject's designated initializer.
            unsafe { msg_send![super(this), init] }
        }
    }

    pub struct Tray {
        mtm: MainThreadMarker,
        item: Retained<NSStatusItem>,
        menu: Retained<NSMenu>,
        target: Retained<Target>,
        /// The last symbol set, so unchanged refreshes don't touch the icon.
        shown: Option<&'static str>,
    }

    impl Tray {
        /// Must run on the main thread (Rune runs `AppHandle::update` there).
        pub fn new(mtm: MainThreadMarker, on_action: impl Fn(TrayAction) + 'static) -> Self {
            let item =
                NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
            let menu = NSMenu::new(mtm);
            menu.setAutoenablesItems(false);
            item.setMenu(Some(&menu));
            Self {
                mtm,
                item,
                menu,
                target: Target::new(mtm, Box::new(on_action)),
                shown: None,
            }
        }

        /// Updates the icon, tooltip and menu to match `activity`.
        pub fn show(&mut self, activity: &Activity, snap: &Snapshot) {
            let (name, description) = symbol(activity);
            if self.shown != Some(name) {
                if let Some(button) = self.item.button(self.mtm) {
                    match NSImage::imageWithSystemSymbolName_accessibilityDescription(
                        &NSString::from_str(name),
                        Some(&NSString::from_str(description)),
                    ) {
                        Some(image) => {
                            image.setTemplate(true);
                            button.setImage(Some(&image));
                            button.setTitle(&NSString::from_str(""));
                        }
                        // Symbols ship with macOS 11+; fall back to text.
                        None => button.setTitle(&NSString::from_str(match activity {
                            Activity::Blocking { .. } => "◉",
                            Activity::Idle => "○",
                            Activity::Attention(_) => "⚠",
                        })),
                    }
                }
                self.shown = Some(name);
            }
            let headline = activity.headline();
            if let Some(button) = self.item.button(self.mtm) {
                button.setToolTip(Some(&NSString::from_str(&headline)));
            }
            self.rebuild_menu(activity, snap);
        }

        fn rebuild_menu(&self, activity: &Activity, snap: &Snapshot) {
            self.menu.removeAllItems();
            for (i, line) in status_lines(activity, snap).iter().enumerate() {
                let title = if i == 0 {
                    line.clone()
                } else {
                    format!("    {line}")
                };
                let item = self.item_with(&title, None, "");
                item.setEnabled(false);
                self.menu.addItem(&item);
            }
            self.menu.addItem(&NSMenuItem::separatorItem(self.mtm));
            self.menu
                .addItem(&self.item_with("Open Blocker…", Some(sel!(open:)), ","));
            self.menu
                .addItem(&self.item_with("Update Lists Now", Some(sel!(updateLists:)), ""));
            self.menu.addItem(&NSMenuItem::separatorItem(self.mtm));
            self.menu
                .addItem(&self.item_with("Quit Blocker", Some(sel!(quit:)), "q"));
        }

        fn item_with(
            &self,
            title: &str,
            action: Option<objc2::runtime::Sel>,
            key: &str,
        ) -> Retained<NSMenuItem> {
            // SAFETY: a fresh item with a valid selector (or none) that the
            // target implements.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(self.mtm),
                    &NSString::from_str(title),
                    action,
                    &NSString::from_str(key),
                )
            };
            if action.is_some() {
                // SAFETY: the target lives as long as the tray, which owns the menu.
                unsafe { item.setTarget(Some(&self.target)) };
            }
            item
        }

        /// Menu titles as currently shown, for tests and diagnostics.
        pub fn menu_titles(&self) -> Vec<String> {
            self.menu
                .itemArray()
                .iter()
                .map(|i| i.title().to_string())
                .collect()
        }

        pub fn symbol_shown(&self) -> Option<&'static str> {
            self.shown
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_activity_has_its_own_symbol() {
        let a = symbol(&Activity::Blocking {
            rules: vec!["x".into()],
        })
        .0;
        let b = symbol(&Activity::Idle).0;
        let c = symbol(&Activity::Attention("x".into())).0;
        assert!(a != b && b != c && a != c);
    }

    #[test]
    fn rune_menu_and_events_round_trip() {
        let snap = Snapshot::default();
        let menu = rune_menu(&Activity::Idle, &snap);
        assert_eq!(
            menu[0],
            rune::TrayItem::action("status-0", "Not blocking right now").enabled(false)
        );
        let ids: Vec<&str> = menu
            .iter()
            .filter_map(|i| match i {
                rune::TrayItem::Action {
                    id, enabled: true, ..
                } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, ["open", "update", "quit"]);
        assert_eq!(
            action_for(&rune::TrayEvent::Menu("update".into())),
            Some(TrayAction::UpdateLists)
        );
        assert_eq!(action_for(&rune::TrayEvent::Menu("status-0".into())), None);
        assert_eq!(
            action_for(&rune::TrayEvent::Activate { token: None }),
            Some(TrayAction::Open)
        );
    }

    #[test]
    fn drawn_icons_differ_per_state() {
        let blocking = icon(&Activity::Blocking { rules: vec![] });
        let idle = icon(&Activity::Idle);
        let attention = icon(&Activity::Attention("x".into()));
        assert!(blocking != idle && idle != attention && blocking != attention);
        // The rune tray refuses icons whose pixels don't match their size.
        let spec = rune::TraySpec::new("t", "T", icon(&Activity::Idle));
        assert!(!matches!(
            rune::Tray::new(spec, |_| {}),
            Err(rune::TrayError::Backend(_))
        ));
    }

    #[test]
    fn status_lines_list_active_rules() {
        use crate::model::RuleRow;
        let row = |name: &str, active: bool, locked: bool| RuleRow {
            name: name.into(),
            enabled: true,
            locked,
            active,
            schedule: "mon-fri 09:00-17:00".into(),
            domains: 1,
            categories: vec![],
            apps: 0,
        };
        let snap = Snapshot {
            rules: vec![row("focus", true, true), row("games", false, false)],
            ..Snapshot::default()
        };
        let activity = snap.activity(None);
        assert_eq!(
            status_lines(&activity, &snap),
            vec![
                "Blocking · focus".to_string(),
                "focus 🔒 — mon-fri 09:00-17:00".to_string()
            ]
        );
    }
}
