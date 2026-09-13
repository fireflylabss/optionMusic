use gpui::{BoxShadow, px, rgb, rgba};

#[derive(Clone, Copy)]
pub(crate) struct MusicTokens {
    pub(crate) bg: gpui::Rgba,
    pub(crate) panel: gpui::Rgba,
    pub(crate) elevated: gpui::Rgba,
    pub(crate) lift: gpui::Rgba,
    pub(crate) lift_2: gpui::Rgba,
    pub(crate) border: gpui::Rgba,
    pub(crate) border_strong: gpui::Rgba,
    pub(crate) glow: gpui::Rgba,
    pub(crate) ink: gpui::Rgba,
    pub(crate) ink_2: gpui::Rgba,
    pub(crate) mute: gpui::Rgba,
    pub(crate) faint: gpui::Rgba,
    pub(crate) white: gpui::Rgba,
    pub(crate) black: gpui::Rgba,
    pub(crate) selected: gpui::Rgba,
    pub(crate) hover: gpui::Rgba,
    pub(crate) focus: gpui::Rgba,
}

pub(crate) fn tokens() -> MusicTokens {
    MusicTokens {
        bg: rgb(0x0c0c0c),
        panel: rgb(0x141414),
        elevated: rgb(0x1c1c1c),
        lift: rgb(0x242424),
        lift_2: rgb(0x2e2e2e),
        border: rgba(0xffffff0e),
        border_strong: rgba(0xffffff1a),
        glow: rgba(0xffffff14),
        ink: rgb(0xffffff),
        ink_2: rgba(0xffffffc0),
        mute: rgb(0x9ca3af),
        faint: rgba(0x9ca3af90),
        white: rgb(0xffffff),
        black: rgb(0x0c0c0c),
        selected: rgba(0xffffff08),
        hover: rgba(0xffffff0c),
        focus: rgb(0x555555),
    }
}

pub(crate) fn cover_shadow(tokens: MusicTokens) -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(10.), tokens.black.opacity(0.5).into())
            .blur_radius(px(24.))
            .spread_radius(px(-4.)),
        BoxShadow::new(px(0.), px(4.), tokens.black.opacity(0.3).into()).blur_radius(px(8.)),
    ]
}

/// Anchored popovers (context menu, pickers): deep drop shadow + a hairline
/// top highlight so the panel separates from the content behind it.
pub(crate) fn menu_shadow(tokens: MusicTokens) -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(10.), tokens.black.opacity(0.6).into()).blur_radius(px(30.)),
        BoxShadow::new(px(0.), px(0.), tokens.white.opacity(0.06).into()).blur_radius(px(1.)),
    ]
}

/// Centered modal dialogs: one more layer of depth than popovers.
pub(crate) fn dialog_shadow(tokens: MusicTokens) -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(18.), tokens.black.opacity(0.7).into()).blur_radius(px(48.)),
        BoxShadow::new(px(0.), px(4.), tokens.black.opacity(0.45).into()).blur_radius(px(14.)),
        BoxShadow::new(px(0.), px(0.), tokens.white.opacity(0.08).into()).blur_radius(px(1.)),
    ]
}

/// Keyboard-focus ring: a crisp 1.5px outline + soft halo. Spread-only shadows
/// draw a visible ring without changing the element's box (no layout shift),
/// unlike `border_color` which renders nothing without an explicit border.
pub(crate) fn focus_shadow(tokens: MusicTokens) -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(0.), tokens.ink_2.into()).spread_radius(px(1.5)),
        BoxShadow::new(px(0.), px(0.), tokens.glow.opacity(1.0).into()).blur_radius(px(8.)),
    ]
}
