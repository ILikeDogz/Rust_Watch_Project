extern crate alloc;

use alloc::vec::Vec;
use core::cell::RefCell;
use critical_section::Mutex;

// uses a simple stack for navigation history
static NAV_HISTORY: Mutex<RefCell<Vec<Page>>> = Mutex::new(RefCell::new(Vec::new()));

fn nav_push(p: Page) {
    critical_section::with(|cs| {
        NAV_HISTORY.borrow(cs).borrow_mut().push(p);
    });
}

fn nav_pop() -> Option<Page> {
    critical_section::with(|cs| NAV_HISTORY.borrow(cs).borrow_mut().pop())
}

pub fn clear_nav() {
    critical_section::with(|cs| {
        NAV_HISTORY.borrow(cs).borrow_mut().clear();
    });
}

// UI State representation
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct UiState {
    pub page: Page,
    pub dialog: Option<Dialog>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Main(MainMenuState),
    Watch(WatchAppState),
    Settings(SettingsMenuState),
    Omnitrix(OmnitrixState),
    EasterEgg,
}

// Dialogs that can overlay on top of pages
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    TransformPage,
}

// States for Main Menu
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MainMenuState {
    Home,        // just show home
    WatchApp,    // enter watch app (analog/digital)
    SettingsApp, // enter Settings
}

// States for Watch App
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WatchAppState {
    Analog,
    Digital,
}

// States for Settings Menu
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SettingsMenuState {
    BrightnessPrompt,
    BrightnessAdjust,
    EasterEgg,
}

// States for Omnitrix Menu
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OmnitrixState {
    Alien1,
    Alien2,
    Alien3,
    Alien4,
    Alien5,
    Alien6,
    Alien7,
    Alien8,
    Alien9,
    Alien10,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum PageKind {
    Main,
    Settings,
    Omnitrix,
    EasterEgg,
    Watch,
}

impl UiState {
    // Move to the next item/state in the current layer (rotary CW)
    pub fn next_item(self) -> Self {
        if self.dialog.is_some() {
            return self;
        }
        let next_page = match self.page {
            Page::Main(state) => {
                let next = match state {
                    MainMenuState::Home => MainMenuState::WatchApp,
                    MainMenuState::WatchApp => MainMenuState::SettingsApp,
                    MainMenuState::SettingsApp => MainMenuState::Home,
                };
                Page::Main(next)
            }
            Page::Watch(state) => {
                let next = match state {
                    WatchAppState::Analog => WatchAppState::Digital,
                    WatchAppState::Digital => WatchAppState::Analog,
                };
                Page::Watch(next)
            }
            Page::Settings(state) => {
                let next = match state {
                    SettingsMenuState::BrightnessPrompt => SettingsMenuState::EasterEgg,
                    SettingsMenuState::EasterEgg => SettingsMenuState::BrightnessPrompt,
                    SettingsMenuState::BrightnessAdjust => SettingsMenuState::BrightnessAdjust,
                };
                Page::Settings(next)
            }
            Page::Omnitrix(state) => {
                let next = match state {
                    OmnitrixState::Alien1 => OmnitrixState::Alien2,
                    OmnitrixState::Alien2 => OmnitrixState::Alien3,
                    OmnitrixState::Alien3 => OmnitrixState::Alien4,
                    OmnitrixState::Alien4 => OmnitrixState::Alien5,
                    OmnitrixState::Alien5 => OmnitrixState::Alien6,
                    OmnitrixState::Alien6 => OmnitrixState::Alien7,
                    OmnitrixState::Alien7 => OmnitrixState::Alien8,
                    OmnitrixState::Alien8 => OmnitrixState::Alien9,
                    OmnitrixState::Alien9 => OmnitrixState::Alien10,
                    OmnitrixState::Alien10 => OmnitrixState::Alien1,
                };
                Page::Omnitrix(next)
            }
            Page::EasterEgg => Page::EasterEgg,
        };
        Self {
            page: next_page,
            dialog: None,
        }
    }

    // Move to the previous item/state (rotary CCW)
    pub fn prev_item(self) -> Self {
        if self.dialog.is_some() {
            return self;
        }
        let prev_page = match self.page {
            Page::Main(state) => {
                let prev = match state {
                    MainMenuState::Home => MainMenuState::SettingsApp,
                    MainMenuState::WatchApp => MainMenuState::Home,
                    MainMenuState::SettingsApp => MainMenuState::WatchApp,
                };
                Page::Main(prev)
            }
            Page::Watch(state) => {
                let prev = match state {
                    WatchAppState::Analog => WatchAppState::Digital,
                    WatchAppState::Digital => WatchAppState::Analog,
                };
                Page::Watch(prev)
            }
            Page::Settings(state) => {
                let prev = match state {
                    SettingsMenuState::BrightnessPrompt => SettingsMenuState::EasterEgg,
                    SettingsMenuState::EasterEgg => SettingsMenuState::BrightnessPrompt,
                    SettingsMenuState::BrightnessAdjust => SettingsMenuState::BrightnessAdjust,
                };
                Page::Settings(prev)
            }
            Page::Omnitrix(state) => {
                let prev = match state {
                    OmnitrixState::Alien1 => OmnitrixState::Alien10,
                    OmnitrixState::Alien2 => OmnitrixState::Alien1,
                    OmnitrixState::Alien3 => OmnitrixState::Alien2,
                    OmnitrixState::Alien4 => OmnitrixState::Alien3,
                    OmnitrixState::Alien5 => OmnitrixState::Alien4,
                    OmnitrixState::Alien6 => OmnitrixState::Alien5,
                    OmnitrixState::Alien7 => OmnitrixState::Alien6,
                    OmnitrixState::Alien8 => OmnitrixState::Alien7,
                    OmnitrixState::Alien9 => OmnitrixState::Alien8,
                    OmnitrixState::Alien10 => OmnitrixState::Alien9,
                };
                Page::Omnitrix(prev)
            }
            Page::EasterEgg => Page::EasterEgg,
        };
        Self {
            page: prev_page,
            dialog: None,
        }
    }

    // Go back (Button 1)
    pub fn back(self) -> Self {
        if self.dialog.is_some() {
            return Self {
                page: self.page,
                dialog: None,
            };
        }
        // If in Settings adjust view, pop back to prompt (also pop nav once).
        if matches!(
            self.page,
            Page::Settings(SettingsMenuState::BrightnessAdjust)
        ) {
            let _ = nav_pop();
            return Self {
                page: Page::Settings(SettingsMenuState::BrightnessPrompt),
                dialog: None,
            };
        }
        if matches!(self.page, Page::EasterEgg) {
            let _ = nav_pop(); // drop the settings->easter egg push
            return Self {
                page: Page::Settings(SettingsMenuState::EasterEgg),
                dialog: None,
            };
        }

        // Otherwise, try navigation history first.
        if let Some(prev) = nav_pop() {
            return Self {
                page: prev,
                dialog: None,
            };
        }
        // Fallback if no history
        Self {
            page: Page::Main(MainMenuState::Home),
            dialog: None,
        }
    }

    // Select/enter (Button 2)
    pub fn select(self) -> Self {
        if let Some(_) = self.dialog {
            return Self {
                page: self.page,
                dialog: None,
            };
        }
        match self.page {
            Page::Main(state) => {
                nav_push(Page::Main(state));
                let page = match state {
                    MainMenuState::Home => Page::Omnitrix(OmnitrixState::Alien1),
                    MainMenuState::WatchApp => Page::Watch(WatchAppState::Analog),
                    MainMenuState::SettingsApp => {
                        Page::Settings(SettingsMenuState::BrightnessPrompt)
                    }
                };
                Self { page, dialog: None }
            }
            Page::Watch(_) => Self {
                page: self.page,
                dialog: None,
            },
            Page::Settings(s) => {
                let page = match s {
                    SettingsMenuState::BrightnessPrompt => {
                        nav_push(Page::Settings(s));
                        Page::Settings(SettingsMenuState::BrightnessAdjust)
                    }
                    SettingsMenuState::EasterEgg => {
                        nav_push(Page::Settings(s));
                        Page::EasterEgg
                    }
                    _ => self.page,
                };
                Self { page, dialog: None }
            }
            Page::Omnitrix(_) => Self {
                page: self.page,
                dialog: None,
            }, // changed
            Page::EasterEgg => Self {
                page: self.page,
                dialog: None,
            },
        }
    }

    // Omnitrix transform (Button 3)
    pub fn transform(self) -> Self {
        // Only if on Omnitrix and no dialog already
        if matches!(self.page, Page::Omnitrix(_)) && self.dialog.is_none() {
            Self {
                page: self.page,
                dialog: Some(Dialog::TransformPage),
            }
        } else {
            self
        }
    }
}
