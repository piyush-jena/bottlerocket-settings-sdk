//! Settings related to Kubelet Device Plugins
use bottlerocket_model_derive::model;
use bottlerocket_modeled_types::NvidiaDevicePluginSettings;
use bottlerocket_settings_sdk::{GenerateResult, SettingsModel};
use std::convert::Infallible;

#[model(impl_default = true)]
pub struct KubeletDevicePluginsV1 {
    nvidia: NvidiaDevicePluginSettings,
}

type Result<T> = std::result::Result<T, Infallible>;

impl SettingsModel for KubeletDevicePluginsV1 {
    type PartialKind = Self;
    type ErrorKind = Infallible;

    fn get_version() -> &'static str {
        "v1"
    }

    fn set(_current_value: Option<Self>, _target: Self) -> Result<()> {
        // Set anything that can be parsed as ECSSettingsV1.
        Ok(())
    }

    fn generate(
        existing_partial: Option<Self::PartialKind>,
        _dependent_settings: Option<serde_json::Value>,
    ) -> Result<GenerateResult<Self::PartialKind, Self>> {
        Ok(GenerateResult::Complete(
            existing_partial.unwrap_or_default(),
        ))
    }

    fn validate(_value: Self, _validated_settings: Option<serde_json::Value>) -> Result<()> {
        // KubeletDevicePluginsV1 is validated during deserialization.
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bottlerocket_modeled_types::{
        NvidiaDeviceIdStrategy, NvidiaDeviceListStrategy, NvidiaDeviceSharingStrategy,
        NvidiaDevicePartitioningStrategy, NvidiaTimeSlicingSettings, NvidiaMIGSettings,
        MIGA100Profile, MIGH100Profile, MIGH200Profile,
    };
    use bounded_integer::BoundedI32;

    #[test]
    fn test_generate_kubelet_device_plugins() {
        let generated = KubeletDevicePluginsV1::generate(None, None).unwrap();
        assert_eq!(
            generated,
            GenerateResult::Complete(KubeletDevicePluginsV1 { nvidia: None })
        );
    }

    #[test]
    fn test_serde_kubelet_device_plugins() {
        let test_json = r#"{"nvidia":{"pass-device-specs":true,"device-id-strategy":"index","device-list-strategy":"volume-mounts","device-sharing-strategy":"time-slicing","device-partitioning-strategy":"mig","time-slicing":{"replicas":2,"rename-by-default":true,"fail-requests-greater-than-one":true},"mig":{"profile-a100":"1g.5gb","profile-h100":"7g.80gb","profile-h200":"7g.141gb"}}}"#;

        let device_plugins: KubeletDevicePluginsV1 = serde_json::from_str(test_json).unwrap();
        assert_eq!(
            device_plugins,
            KubeletDevicePluginsV1 {
                nvidia: Some(NvidiaDevicePluginSettings {
                    pass_device_specs: Some(true),
                    device_id_strategy: Some(NvidiaDeviceIdStrategy::Index),
                    device_list_strategy: Some(NvidiaDeviceListStrategy::VolumeMounts),
                    device_sharing_strategy: Some(NvidiaDeviceSharingStrategy::TimeSlicing),
                    device_partitioning_strategy: Some(NvidiaDevicePartitioningStrategy::MIG),
                    time_slicing: Some(NvidiaTimeSlicingSettings {
                        replicas: Some(BoundedI32::new(2).unwrap()),
                        rename_by_default: Some(true),
                        fail_requests_greater_than_one: Some(true),
                    }),
                    mig: Some(NvidiaMIGSettings {
                        profile_a100: Some(MIGA100Profile::try_from("1g.5gb").unwrap()),
                        profile_h100: Some(MIGH100Profile::try_from("7g.80gb").unwrap()),
                        profile_h200: Some(MIGH200Profile::try_from("7g.141gb").unwrap()),
                    }),
                }),
            }
        );

        let results = serde_json::to_string(&device_plugins).unwrap();
        assert_eq!(results, test_json);
    }
}
