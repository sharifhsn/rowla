//! Three explicit system actions; URLs cannot select or close arbitrary windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) enum SystemAction {
    Sort,
    Show,
    Hide,
}
impl SystemAction {
    pub(crate) fn shortcut_name(self) -> &'static str {
        match self {
            Self::Sort => "Rowla Sort Windows",
            Self::Show => "Rowla Show Taskbars",
            Self::Hide => "Rowla Hide Taskbars",
        }
    }

    pub(crate) fn from_url(url: &str) -> Option<Self> {
        match url {
            "rowla://sort" | "rowla://sort/" => Some(Self::Sort),
            "rowla://show" | "rowla://show/" => Some(Self::Show),
            "rowla://hide" | "rowla://hide/" => Some(Self::Hide),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_actions_accept_only_the_three_parameter_free_commands() {
        for (url, expected) in [
            ("rowla://sort", SystemAction::Sort),
            ("rowla://show/", SystemAction::Show),
            ("rowla://hide", SystemAction::Hide),
        ] {
            assert_eq!(SystemAction::from_url(url), Some(expected));
        }
        for url in [
            "https://sort",
            "rowla://quit",
            "rowla://sort?window=1",
            "rowla://show/path",
            "rowla://hide#extra",
            "rowla://user@sort",
        ] {
            assert_eq!(SystemAction::from_url(url), None);
        }
    }
}
