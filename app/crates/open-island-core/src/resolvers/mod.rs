pub mod alacritty;
pub mod ghostty;
pub mod kitty;
pub mod tmux;
pub mod vscode;
pub mod wezterm;
pub mod zed;
pub mod zellij;

use crate::jump::LocationResolver;

pub fn default_resolvers() -> Vec<Box<dyn LocationResolver>> {
    vec![
        Box::new(tmux::TmuxResolver),
        Box::new(zellij::ZellijResolver),
        Box::new(wezterm::WeztermResolver),
        Box::new(kitty::KittyResolver),
        Box::new(ghostty::GhosttyResolver),
        Box::new(alacritty::AlacrittyResolver),
        Box::new(zed::ZedResolver),
        Box::new(vscode::VscodeResolver),
    ]
}
pub mod ancestry;
