#![allow(non_snake_case, missing_docs)]

#[macro_export]
macro_rules! implement_wdk_default {
    ($bindgen_mod:ident) => {
        use $bindgen_mod as wdk_bindgen;

        #[derive(Debug)]
        pub struct WdkCallbacks {
            wdf_function_table_symbol_name: Option<String>,
        }

        impl WdkCallbacks {
            #[must_use]
            pub const fn new(wdf_function_table_symbol_name: Option<String>) -> Self {
                Self {
                    wdf_function_table_symbol_name,
                }
            }
        }

        impl wdk_bindgen::callbacks::ParseCallbacks for WdkCallbacks {
            fn generated_name_override(
                &self,
                item_info: wdk_bindgen::callbacks::ItemInfo<'_>,
            ) -> Option<String> {
                if let Some(wdf_function_table_symbol_name) = &self.wdf_function_table_symbol_name {
                    if matches!(item_info.kind, ::bindgen::callbacks::ItemKind::Var)
                        && item_info.name == wdf_function_table_symbol_name
                    {
                        return Some("WdfFunctions".to_string());
                    }
                }
                None
            }
        }
        
        pub struct WdkBuilder {
            builder: wdk_bindgen::Builder,
        }
        
        impl WdkBuilder {
            pub fn wdk_default(
                wdf_function_table_symbol_name: Option<String>,
                config: &::wdk_build::Config,
            ) -> Result<Self, ::wdk_build::ConfigError> {
                Ok(Self {
                    builder: wdk_bindgen::Builder::default()
                        .use_core()
                        .derive_default(true)
                        .generate_cstr(true)
                        .detect_include_paths(false)
                        .clang_args(config.include_paths()?.map(|include_path| {
                            format!("--include-directory={}", include_path.to_string_lossy())
                        }))
                        .clang_args(
                            config
                                .preprocessor_definitions()
                                .map(|(key, value)| {
                                    format!(
                                        "--define-macro={}{}",
                                        key,
                                        value.map(|v| format!("={v}")).unwrap_or_default()
                                    )
                                })
                                .chain(::wdk_build::Config::wdk_bindgen_compiler_flags()),
                        )
                        .blocklist_item("ExAllocatePoolWithTag")
                        .blocklist_item("ExAllocatePoolWithQuotaTag")
                        .blocklist_item("ExAllocatePoolWithTagPriority")
                        .blocklist_item("ExAllocatePool")
                        .opaque_type("_KGDTENTRY64")
                        .opaque_type("_KIDTENTRY64")
                        .blocklist_item(".*MCG_CAP(?:__bindgen.*)?")
                        .blocklist_item(".*WHEA_XPF_MCA_SECTION")
                        .blocklist_item(".*WHEA_ARM_BUS_ERROR(?:__bindgen.*)?")
                        .blocklist_item(".*WHEA_ARM_PROCESSOR_ERROR")
                        .blocklist_item(".*WHEA_ARM_CACHE_ERROR")
                        .no_default(".*tagMONITORINFOEXA")
                        .must_use_type("NTSTATUS")
                        .must_use_type("HRESULT")
                        .default_enum_style(wdk_bindgen::EnumVariation::ModuleConsts)
                        .parse_callbacks(Box::new(WdkCallbacks::new(wdf_function_table_symbol_name)))
                        .formatter(wdk_bindgen::Formatter::Prettyplease),
                })
            }
        
            #[must_use]
            pub fn builder(self) -> wdk_bindgen::Builder {
                self.builder
            }
        }
        
        /// Extension trait for `bindgen::Builder`
        pub trait WdkBuilderExt {
            fn wdk_default(
                config: &::wdk_build::Config,
            ) -> Result<wdk_bindgen::Builder, ::wdk_build::ConfigError>;
        }
        
        /// Implementation of `wdk_default` for `bindgen::Builder`
        impl WdkBuilderExt for wdk_bindgen::Builder {
            fn wdk_default(
                config: &::wdk_build::Config,
            ) -> Result<wdk_bindgen::Builder, ::wdk_build::ConfigError> {
                Ok(WdkBuilder::wdk_default(
                    Some(::wdk_build::Config::wdk_bindgen_compiler_flags().collect()),
                    config,
                )?
                .builder())
            }
        }
    };
}

pub use implement_wdk_default;
