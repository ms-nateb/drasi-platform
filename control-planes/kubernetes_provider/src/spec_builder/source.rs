// Copyright 2024 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::models::ResourceType;

use super::{
    super::models::{KubernetesSpec, RuntimeConfig},
    build_deployment_spec,
    identity::apply_identity,
    SpecBuilder,
};
use k8s_openapi::{
    api::core::v1::{ServicePort, ServiceSpec},
    apimachinery::pkg::util::intstr::IntOrString,
};
use resource_provider_api::models::{ConfigValue, EndpointSetting, ResourceRequest, SourceSpec};
use std::collections::BTreeMap;

macro_rules! hashmap {
  ($( $key: expr => $val: expr ),*) => {{
       let mut map = ::std::collections::BTreeMap::new();
       $( map.insert($key.to_string(), $val); )*
       map
  }}
}

pub struct SourceSpecBuilder {}

impl SpecBuilder<SourceSpec> for SourceSpecBuilder {
    fn build(
        &self,
        source: ResourceRequest<SourceSpec>,
        runtime_config: &RuntimeConfig,
        instance_id: &str,
    ) -> Vec<KubernetesSpec> {
        let mut specs = Vec::new();

        specs.push(KubernetesSpec {
            resource_type: ResourceType::Source,
            resource_id: source.id.to_string(),
            service_name: "change-router".to_string(),
            deployment: build_deployment_spec(
                runtime_config,
                ResourceType::Source,
                &source.id,
                "change-router",
                "source-change-router",
                false,
                1,
                Some(3000),
                hashmap![
                "SOURCE_ID" => ConfigValue::Inline { value: source.id.clone() },
                "INSTANCE_ID" => ConfigValue::Inline { value: instance_id.to_string() }
                ],
                None,
                None,
                None,
                None,
            ),
            services: BTreeMap::new(),
            config_maps: BTreeMap::new(),
            volume_claims: BTreeMap::new(),
            pub_sub: None,
            service_account: None,
            removed: false,
        });

        specs.push(KubernetesSpec {
            resource_type: ResourceType::Source,
            resource_id: source.id.to_string(),
            service_name: "change-dispatcher".to_string(),
            deployment: build_deployment_spec(
                runtime_config,
                ResourceType::Source,
                &source.id,
                "change-dispatcher",
                "source-change-dispatcher",
                false,
                1,
                Some(3000),
                hashmap![
                "SOURCE_ID" => ConfigValue::Inline { value: source.id.clone() },
                "INSTANCE_ID" => ConfigValue::Inline { value: instance_id.to_string() }
                ],
                None,
                None,
                None,
                None,
            ),
            services: BTreeMap::new(),
            config_maps: BTreeMap::new(),
            volume_claims: BTreeMap::new(),
            pub_sub: None,
            service_account: None,
            removed: false,
        });

        specs.push(KubernetesSpec {
            resource_type: ResourceType::Source,
            resource_id: source.id.to_string(),
            service_name: "query-api".to_string(),
            deployment: build_deployment_spec(
                runtime_config,
                ResourceType::Source,
                &source.id,
                "query-api",
                "source-query-api",
                false,
                1,
                Some(80),
                hashmap![
                "SOURCE_ID" => ConfigValue::Inline { value: source.id.clone() },
                "INSTANCE_ID" => ConfigValue::Inline { value: instance_id.to_string() }
                ],
                Some(hashmap![
                    "api" => 80
                ]),
                None,
                None,
                None,
            ),
            services: hashmap!(
                "query-api".to_string() => ServiceSpec {
                    type_: Some("ClusterIP".to_string()),
                    selector: Some(hashmap![
                        "drasi/type".to_string() => ResourceType::Source.to_string(),
                        "drasi/resource".to_string() => source.id.to_string(),
                        "drasi/service".to_string() => "query-api".to_string()
                    ]),
                    ports: Some(vec![ServicePort {
                        port: 80,
                        target_port: Some(IntOrString::String("api".to_string())),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }
            ),
            config_maps: BTreeMap::new(),
            volume_claims: BTreeMap::new(),
            pub_sub: None,
            service_account: None,
            removed: false,
        });

        let source_spec = source.spec;
        let services = source_spec.services.unwrap_or_default();
        let properties = source_spec.properties.unwrap_or_default();

        let env_var_map: BTreeMap<String, ConfigValue> = properties.into_iter().collect();

        for (service_name, service_spec) in services {
            let app_port = match service_spec.dapr {
                Some(ref dapr) => match dapr.get("app-port") {
                    Some(ConfigValue::Inline { value }) => Some(value.parse::<u16>().unwrap()),
                    _ => None,
                },
                None => None,
            };

            let app_protocol = match service_spec.dapr {
                Some(ref dapr) => match dapr.get("app-protocol") {
                    Some(ConfigValue::Inline { value }) => Some(value.clone()),
                    _ => None,
                },
                None => None,
            };

            let replica = match service_spec.replica {
                Some(rep) => rep.parse::<i32>().unwrap_or(1),
                None => 1,
            };
            let mut env_var_map = env_var_map.clone();
            // combine this with the properties in service_spec
            env_var_map.insert(
                "SOURCE_ID".to_string(),
                ConfigValue::Inline {
                    value: source.id.clone(),
                },
            );

            env_var_map.insert(
                "INSTANCE_ID".to_string(),
                ConfigValue::Inline {
                    value: instance_id.to_string(),
                },
            );

            if let Some(props) = service_spec.properties {
                for (key, value) in props {
                    env_var_map.insert(key, value);
                }
            }
            let mut k8s_services = BTreeMap::new();
            if let Some(endpoints) = service_spec.endpoints {
                for (endpoint_name, endpoint) in endpoints {
                    match endpoint.setting {
                        EndpointSetting::Internal => {
                            let port = endpoint.target.parse::<i32>().unwrap();
                            let service_spec = ServiceSpec {
                                type_: Some("ClusterIP".to_string()),
                                selector: Some(hashmap![
                                    "drasi/type".to_string() => ResourceType::Source.to_string(),
                                    "drasi/resource".to_string() => source.id.clone(),
                                    "drasi/service".to_string() => service_name.clone()
                                ]),
                                ports: Some(vec![ServicePort {
                                    name: Some(endpoint_name.clone()),
                                    port,
                                    target_port: Some(IntOrString::Int(port)),
                                    ..Default::default()
                                }]),
                                ..Default::default()
                            };

                            k8s_services.insert(endpoint_name.clone(), service_spec);
                        }
                        EndpointSetting::External => {
                            unimplemented!();
                        }
                        _ => {
                            unreachable!();
                        }
                    }
                }
            };

            let mut k8s_spec = KubernetesSpec {
                resource_type: ResourceType::Source,
                resource_id: source.id.to_string(),
                service_name: service_name.to_string(),
                deployment: build_deployment_spec(
                    runtime_config,
                    ResourceType::Source,
                    &source.id,
                    &service_name,
                    service_spec.image.as_str(),
                    service_spec.external_image.unwrap_or(false),
                    replica,
                    app_port,
                    env_var_map.clone(),
                    None,
                    None,
                    None,
                    app_protocol,
                ),
                services: k8s_services,
                config_maps: BTreeMap::new(),
                volume_claims: BTreeMap::new(),
                pub_sub: None,
                service_account: None,
                removed: false,
            };

            if service_name == "proxy" {
                apply_proxy_svc(&mut k8s_spec, app_port);
            }

            if let Some(identity) = &source_spec.identity {
                apply_identity(&mut k8s_spec, identity);
            }

            specs.push(k8s_spec);
        }
        specs
    }
}

fn apply_proxy_svc(spec: &mut KubernetesSpec, app_port: Option<u16>) {
    let port = app_port.unwrap_or(80);
    let svc = ServiceSpec {
        type_: Some("ClusterIP".to_string()),
        selector: Some(hashmap![
            "drasi/type".to_string() => spec.resource_type.to_string(),
            "drasi/resource".to_string() => spec.resource_id.to_string(),
            "drasi/service".to_string() => spec.service_name.to_string()
        ]),
        ports: Some(vec![ServicePort {
            port: 80,
            target_port: Some(IntOrString::Int(port.into())),
            ..Default::default()
        }]),
        ..Default::default()
    };
    spec.services.insert("proxy".to_string(), svc);
}
