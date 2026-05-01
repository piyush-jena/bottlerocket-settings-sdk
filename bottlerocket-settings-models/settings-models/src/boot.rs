//! Boot config settings structures.
use bottlerocket_model_derive::model;
use bottlerocket_modeled_types::{BootConfigKey, BootConfigValue};
use indexmap::IndexMap;
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

// Kernel boot settings
#[model(impl_default = true)]
pub struct BootSettingsV1 {
    reboot_to_reconcile: bool,
    #[serde(
        alias = "kernel",
        rename(serialize = "kernel"),
        default,
        skip_serializing_if = "Option::is_none"
    )]
    kernel_parameters: OrderedBootConfig,
    #[serde(
        alias = "init",
        rename(serialize = "init"),
        default,
        skip_serializing_if = "Option::is_none"
    )]
    init_parameters: OrderedBootConfig,
}

/// An insertion-order-preserving collection of kernel/init boot-config parameters.
///
/// # Why this is not a plain `IndexMap`
///
/// The kernel requires some boot-config parameters to be emitted in a specific order — most
/// notably `hugepagesz` must precede `hugepages`, otherwise the kernel reserves zero huge pages.
/// Users express the desired order through the order of keys in their settings, e.g.
///
/// ```toml
/// [settings.boot.kernel-parameters]
/// "hugepagesz" = ["1G"]
/// "hugepages"  = ["10"]
/// ```
///
/// An `IndexMap` preserves insertion order *in memory*, but the Bottlerocket datastore does not
/// preserve the order of a *map* field: when the settings model is written to the datastore each
/// map entry is exploded into its own key (`settings.boot.kernel.hugepagesz`,
/// `settings.boot.kernel.hugepages`, ...) with no ordinal, and on read the keys come back from an
/// unordered `HashSet`. That destroys the user's ordering before the kernel command line / bootconfig
/// is ever rendered.
///
/// To survive that round trip, this type **always serializes as a *sequence*** of `{key, value}`
/// entries rather than as a map. `serde_json::to_value` therefore produces a JSON array, which the
/// datastore stores as a *single ordered value* under one key (`settings.boot.kernel`) and reads
/// back with the order intact.
///
/// For ergonomics and backwards compatibility it still **deserializes from *either*** form:
/// * a **map** — the shape used in user data (`[settings.boot.kernel-parameters]`), in the JSON
///   object accepted/returned by the API, and emitted by the settings generator; or
/// * a **sequence** — the shape persisted in (and read back from) the datastore.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrderedBootConfig(pub IndexMap<BootConfigKey, Vec<BootConfigValue>>);

/// Borrowed view of a single boot-config entry, used to serialize the sequence form without
/// cloning.
#[derive(Serialize)]
struct BootConfigEntryRef<'a> {
    key: &'a BootConfigKey,
    value: &'a [BootConfigValue],
}

/// Owned single boot-config entry, used to deserialize the sequence form.
#[derive(Deserialize)]
struct BootConfigEntry {
    key: BootConfigKey,
    value: Vec<BootConfigValue>,
}

impl Serialize for OrderedBootConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Always serialize as a sequence so the datastore stores this as a single, ordered value
        // (one key) instead of exploding it into per-entry keys that lose ordering.
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            seq.serialize_element(&BootConfigEntryRef { key, value })?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for OrderedBootConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OrderedBootConfigVisitor;

        impl<'de> Visitor<'de> for OrderedBootConfigVisitor {
            type Value = OrderedBootConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(
                    "a map of boot-config keys to value lists, or a sequence of {key, value} entries",
                )
            }

            // Map form: user data, API JSON object, settings-generator output.
            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut map = IndexMap::with_capacity(access.size_hint().unwrap_or(0));
                while let Some((key, value)) =
                    access.next_entry::<BootConfigKey, Vec<BootConfigValue>>()?
                {
                    map.insert(key, value);
                }
                Ok(OrderedBootConfig(map))
            }

            // Sequence form: the representation persisted in the datastore.
            fn visit_seq<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut map = IndexMap::with_capacity(access.size_hint().unwrap_or(0));
                while let Some(entry) = access.next_element::<BootConfigEntry>()? {
                    map.insert(entry.key, entry.value);
                }
                Ok(OrderedBootConfig(map))
            }
        }

        // `deserialize_any` lets the underlying format (serde_json map vs. array, toml table)
        // pick the appropriate visitor method.
        deserializer.deserialize_any(OrderedBootConfigVisitor)
    }
}

#[cfg(test)]
mod test {
    use super::{BootSettingsV1, OrderedBootConfig};
    use bottlerocket_modeled_types::{BootConfigKey, BootConfigValue};
    use indexmap::IndexMap;
    use std::convert::TryInto;

    /// Build an `OrderedBootConfig` from `(key, [values])` pairs, preserving the given order.
    fn ordered(pairs: &[(&str, &[&str])]) -> OrderedBootConfig {
        let mut map: IndexMap<BootConfigKey, Vec<BootConfigValue>> = IndexMap::new();
        for (k, vs) in pairs {
            map.insert(
                (*k).try_into().unwrap(),
                vs.iter().map(|v| (*v).try_into().unwrap()).collect(),
            );
        }
        OrderedBootConfig(map)
    }

    /// The keys, in iteration order, as plain strings.
    fn keys(cfg: &OrderedBootConfig) -> Vec<String> {
        cfg.0.keys().map(|k| k.to_string()).collect()
    }

    #[test]
    fn serializes_as_ordered_sequence() {
        // The whole point: serde_json::to_value must produce a JSON *array* (so the datastore
        // stores it as one ordered value), with entries in insertion order.
        let cfg = ordered(&[("hugepagesz", &["1G"]), ("hugepages", &["10"])]);
        let value = serde_json::to_value(&cfg).unwrap();
        assert_eq!(
            value,
            serde_json::json!([
                {"key": "hugepagesz", "value": ["1G"]},
                {"key": "hugepages",  "value": ["10"]},
            ])
        );
    }

    #[test]
    fn deserializes_from_map_preserving_order() {
        // User-data / API map form. Keys intentionally NOT alphabetical.
        let value = serde_json::json!({
            "hugepagesz": ["1G"],
            "hugepages": ["10"],
            "transparent_hugepage": ["never"],
        });
        let cfg: OrderedBootConfig = serde_json::from_value(value).unwrap();
        assert_eq!(keys(&cfg), vec!["hugepagesz", "hugepages", "transparent_hugepage"]);
    }

    #[test]
    fn deserializes_from_sequence_preserving_order() {
        // Datastore blob form.
        let value = serde_json::json!([
            {"key": "hugepagesz", "value": ["1G"]},
            {"key": "hugepages",  "value": ["10"]},
        ]);
        let cfg: OrderedBootConfig = serde_json::from_value(value).unwrap();
        assert_eq!(keys(&cfg), vec!["hugepagesz", "hugepages"]);
    }

    #[test]
    fn datastore_round_trip_preserves_order() {
        // Mirror exactly what the datastore does to a sequence value: it stores each element by
        // JSON-stringifying it (FlatSerializer) and persists the array as a single string, then
        // parses that string back on read. So `to_value` -> `to_string` -> `from_str` is a
        // faithful proxy for the model -> datastore -> model trip for this field.
        let original = ordered(&[
            ("hugepagesz", &["1G"]),
            ("hugepages", &["10"]),
            ("console", &["ttyS1,115200n8", "tty0"]),
        ]);

        let as_value = serde_json::to_value(&original).unwrap();
        assert!(as_value.is_array(), "must serialize as a single ordered array");

        let blob = serde_json::to_string(&as_value).unwrap();
        let restored: OrderedBootConfig = serde_json::from_str(&blob).unwrap();

        assert_eq!(restored, original);
        assert_eq!(
            keys(&restored),
            vec!["hugepagesz", "hugepages", "console"],
            "insertion order must survive the datastore round trip"
        );
    }

    #[test]
    fn boot_settings_round_trip_via_value() {
        // Full struct: deserialize the user-data map form, then confirm the value form (what
        // `serde_json::to_value` feeds to `to_pairs`) keeps `hugepagesz` before `hugepages`.
        let toml_input = r#"
            [kernel]
            hugepagesz = ["1G"]
            hugepages = ["10"]
            [init]
            splash = []
        "#;
        let settings: BootSettingsV1 = toml::from_str(toml_input).unwrap();

        let value = serde_json::to_value(&settings).unwrap();
        let kernel = value.get("kernel").unwrap().as_array().unwrap();
        let order: Vec<&str> = kernel
            .iter()
            .map(|e| e.get("key").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(order, vec!["hugepagesz", "hugepages"]);

        // And it must round-trip back into an equivalent struct from that value form.
        let restored: BootSettingsV1 = serde_json::from_value(value).unwrap();
        assert_eq!(restored, settings);
    }

    #[test]
    fn empty_is_default() {
        let cfg = OrderedBootConfig::default();
        assert!(cfg.0.is_empty());
        let value = serde_json::to_value(&cfg).unwrap();
        assert_eq!(value, serde_json::json!([]));
    }
}
