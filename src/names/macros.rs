//! The `wire_param_enum!` macro for generating typed parameter enums.

/// Generate a typed wire-property enum with `from_known_name`, `from_name`,
/// `as_str`, and `Display`.
///
/// Unknown names are captured by the `Other(String)` variant for forward compatibility.
macro_rules! wire_param_enum {
    (
        $(#[$enum_doc:meta])*
        $Name:ident { $($variant:ident => $wire:literal),+ $(,)? }
    ) => {
        $(#[$enum_doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $Name {
            $($variant,)+
            #[doc = concat!(
                "A property without a typed [`", stringify!($Name),
                "`] identifier yet. Holds the raw wire name; the value still ",
                "rides in the carrying event/command."
            )]
            Other(String),
        }

        impl $Name {
            #[doc = concat!(
                "Map a wire property name to its typed [`", stringify!($Name),
                "`] variant if known, without allocating."
            )]
            pub fn from_known_name(name: &str) -> Option<$Name> {
                match name {
                    $($wire => Some($Name::$variant),)+
                    _ => None,
                }
            }

            #[doc = concat!(
                "Check if a wire property name matches a known [`", stringify!($Name),
                "`] variant without allocating."
            )]
            pub fn is_known_name(name: &str) -> bool {
                Self::from_known_name(name).is_some()
            }

            #[doc = concat!(
                "Map a wire property name to its typed [`", stringify!($Name),
                "`] variant. Unknown names map to [`", stringify!($Name),
                "::Other`], so this is total (never fails)."
            )]
            pub fn from_name(name: &str) -> $Name {
                Self::from_known_name(name).unwrap_or_else(|| $Name::Other(name.to_string()))
            }

            #[doc = concat!(
                "The wire property name for this parameter (inverse of [`",
                stringify!($Name), "::from_name`])."
            )]
            pub fn as_str(&self) -> &str {
                match self {
                    $($Name::$variant => $wire,)+
                    $Name::Other(s) => s.as_str(),
                }
            }

            #[doc = concat!(
                "`true` if this is a typed variant (not [`", stringify!($Name),
                "::Other`])."
            )]
            pub fn is_known(&self) -> bool {
                !matches!(self, $Name::Other(_))
            }
        }

        impl ::core::fmt::Display for $Name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

#[cfg(test)]
mod tests {
    wire_param_enum! {
        /// Test parameter family.
        DummyParam {
            Alpha => "alphaProp",
            Beta => "betaProp",
        }
    }

    #[test]
    fn from_known_name_and_is_known() {
        assert_eq!(
            DummyParam::from_known_name("alphaProp"),
            Some(DummyParam::Alpha)
        );
        assert_eq!(
            DummyParam::from_known_name("betaProp"),
            Some(DummyParam::Beta)
        );
        assert_eq!(DummyParam::from_known_name("unknownProp"), None);

        assert!(DummyParam::is_known_name("alphaProp"));
        assert!(DummyParam::is_known_name("betaProp"));
        assert!(!DummyParam::is_known_name("unknownProp"));

        assert_eq!(DummyParam::from_name("alphaProp"), DummyParam::Alpha);
        assert_eq!(
            DummyParam::from_name("unknownProp"),
            DummyParam::Other("unknownProp".to_string())
        );
        assert!(DummyParam::Alpha.is_known());
        assert!(!DummyParam::Other("unknownProp".to_string()).is_known());
        assert_eq!(DummyParam::Alpha.as_str(), "alphaProp");
        assert_eq!(DummyParam::Alpha.to_string(), "alphaProp");
    }
}
