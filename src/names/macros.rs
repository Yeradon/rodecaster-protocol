//! The `wire_param_enum!` macro: one list generates a typed wire-property enum
//! plus its name <-> variant codec, shared by every node-scoped property family.

/// Generate a typed wire-property enum plus its name <-> variant codec from one
/// list, so `from_name` and `as_str` can never drift apart. Each entry is
/// `Variant => "wireName"`.
///
/// Every generated enum is `#[non_exhaustive]`, carries an `Other(String)`
/// escape hatch (a property the crate does not yet name surfaces there rather
/// than being dropped, so forward-compatibility is total), and implements
/// [`fmt::Display`] as its wire name. Each variant is a **ground-truth wire
/// name** (the literal JUCE property identifier the firmware uses), not a guess.
/// The *value* of a parameter is carried separately as a self-describing
/// [`crate::Value`], so this enum never has to encode a per-parameter type that
/// could be wrong. The caller supplies the enum's own doc comment.
///
/// One macro spans every node-scoped property family ([`ChannelParam`],
/// [`InputSourceParam`], [`MasterParam`], [`OutputParam`], ...): the families
/// address different nodes but share an identical name-codec shape.
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
                "`] variant. Unknown names map to [`", stringify!($Name),
                "::Other`], so this is total (never fails)."
            )]
            pub fn from_name(name: &str) -> $Name {
                match name {
                    $($wire => $Name::$variant,)+
                    other => $Name::Other(other.to_string()),
                }
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

        impl fmt::Display for $Name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}
