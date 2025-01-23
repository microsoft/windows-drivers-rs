// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::borrow::Borrow;

use bindgen::{
    callbacks::{ItemInfo, ItemKind, ParseCallbacks},
    Builder,
};

use crate::{Config, ConfigError};

/// An extension trait that provides a way to create a [`bindgen::Builder`]
/// configured for generating bindings to the wdk
pub trait BuilderExt {
    /// Returns a `bindgen::Builder` with the default configuration for
    /// generation of bindings to the WDK
    ///
    /// # Errors
    ///
    /// Implementation may return `wdk_build::ConfigError` if it fails to create
    /// a builder
    fn wdk_default(
        c_header_files: Vec<&str>,
        config: impl Borrow<Config>,
    ) -> Result<Builder, ConfigError>;
}

#[derive(Debug)]
struct WdkCallbacks {
    wdf_function_table_symbol_name: Option<String>,
}

impl BuilderExt for Builder {
    /// Returns a `bindgen::Builder` with the default configuration for
    /// generation of bindings to the WDK
    ///
    /// # Errors
    ///
    /// Will return `wdk_build::ConfigError` if any of the resolved include or
    /// library paths do not exist
    fn wdk_default(
        c_header_files: Vec<&str>,
        config: impl Borrow<Config>,
    ) -> Result<Self, ConfigError> {
        let config = config.borrow();

        let mut builder = Self::default();

        for c_header in c_header_files {
            builder = builder.header(c_header);
        }

        builder = builder
            .use_core() // Can't use std for kernel code
            .derive_default(true) // allows for default initializing structs
            // CStr types are safer and easier to work with when interacting with string constants
            // from C
            .generate_cstr(true)
            // Building in eWDK can pollute system search path when clang-sys tries to detect
            // c_search_paths
            .detect_include_paths(false)
            .clang_args(config.get_include_paths()?.iter().map(|include_path| {
                format!(
                    "--include-directory={}",
                    include_path
                        .to_str()
                        .expect("Non Unicode paths are not supported")
                )
            }))
            .clang_args(
                config
                    .get_preprocessor_definitions_iter()
                    .map(|(key, value)| {
                        format!(
                            "--define-macro={key}{}",
                            value.map(|v| format!("={v}")).unwrap_or_default()
                        )
                    })
                    .chain(Config::wdk_bindgen_compiler_flags()),
            )
            .blocklist_item("ExAllocatePoolWithTag") // Deprecated
            .blocklist_item("ExAllocatePoolWithQuotaTag") // Deprecated
            .blocklist_item("ExAllocatePoolWithTagPriority") // Deprecated
            .blocklist_item("ExAllocatePool") // Deprecated
            // FIXME: bitfield generated with non-1byte alignment in _MCG_CAP
            .blocklist_item(".*MCG_CAP(?:__bindgen.*)?")
            .blocklist_item(".*WHEA_XPF_MCA_SECTION")
            .blocklist_item(".*WHEA_ARM_BUS_ERROR(?:__bindgen.*)?")
            .blocklist_item(".*WHEA_ARM_PROCESSOR_ERROR")
            .blocklist_item(".*WHEA_ARM_CACHE_ERROR")
            // Declare handle types defined with DECLARE_HANDLE as opaque
            // wdm.h
            .opaque_type("POHANDLE__")
            .opaque_type("PO_EPM_HANDLE__")
            // wdf_types.h
            .opaque_type("WDFDRIVER__")
            .opaque_type("WDFDEVICE__")
            .opaque_type("WDFWMIPROVIDER__")
            .opaque_type("WDFWMIINSTANCE__")
            .opaque_type("WDFQUEUE__")
            .opaque_type("WDFREQUEST__")
            .opaque_type("WDFFILEOBJECT__")
            .opaque_type("WDFDPC__")
            .opaque_type("WDFTIMER__")
            .opaque_type("WDFWORKITEM__")
            .opaque_type("WDFINTERRUPT__")
            .opaque_type("WDFWAITLOCK__")
            .opaque_type("WDFSPINLOCK__")
            .opaque_type("WDFMEMORY__")
            .opaque_type("WDFLOOKASIDE__")
            .opaque_type("WDFIOTARGET__")
            .opaque_type("WDFUSBDEVICE__")
            .opaque_type("WDFUSBINTERFACE__")
            .opaque_type("WDFUSBPIPE__")
            .opaque_type("WDFDMAENABLER__")
            .opaque_type("WDFDMATRANSACTION__")
            .opaque_type("WDFCOMMONBUFFER__")
            .opaque_type("WDFKEY__")
            .opaque_type("WDFSTRING__")
            .opaque_type("WDFCOLLECTION__")
            .opaque_type("WDFCHILDLIST__")
            .opaque_type("WDFIORESREQLIST__")
            .opaque_type("WDFIORESLIST__")
            .opaque_type("WDFCMRESLIST__")
            .opaque_type("WDFCOMPANION__")
            .opaque_type("WDFTASKQUEUE__")
            .opaque_type("WDFCOMPANIONTARGET__")
            // minwindef.h
            .opaque_type("HKEY__")
            .opaque_type("HMETAFILE__")
            .opaque_type("HINSTANCE__")
            .opaque_type("HRGN__")
            .opaque_type("HRSRC__")
            .opaque_type("HSPRITE__")
            .opaque_type("HLSURF__")
            .opaque_type("HSTR__")
            .opaque_type("HTASK__")
            .opaque_type("HWINSTA__")
            .opaque_type("HKL__")
            //windef.h
            .opaque_type("HWND__")
            .opaque_type("HHOOK__")
            .opaque_type("HEVENT__")
            .opaque_type("HGDIOBJ__")
            .opaque_type("HACCEL__")
            .opaque_type("HBITMAP__")
            .opaque_type("HBRUSH__")
            .opaque_type("HCOLORSPACE__")
            .opaque_type("HDC__")
            .opaque_type("HGLRC__")    
            .opaque_type("HDESK__")
            .opaque_type("HENHMETAFILE__")
            .opaque_type("HFONT__")
            .opaque_type("HICON__")
            .opaque_type("HMENU__")
            .opaque_type("HPALETTE__")
            .opaque_type("HPEN__")
            .opaque_type("HWINEVENTHOOK__")
            .opaque_type("HMONITOR__")
            .opaque_type("HUMPD__")
            .opaque_type("HCURSOR__")    
            .opaque_type("DPI_AWARENESS_CONTEXT__")
            // WinUser.h
            .opaque_type("HTOUCHINPUT__")
            .opaque_type("HSYNTHETICPOINTERDEVICE__")
            .opaque_type("HRAWINPUT__")
            .opaque_type("HGESTUREINFO__")
            // WinNls.h
            .opaque_type("HSAVEDUILANGUAGES__")
            // FIXME: arrays with more than 32 entries currently fail to generate a `Default`` impl: https://github.com/rust-lang/rust-bindgen/issues/2803
            .no_default(".*tagMONITORINFOEXA")
            .must_use_type("NTSTATUS")
            .must_use_type("HRESULT")
            // Defaults enums to generate as a set of constants contained in a module (default value
            // is EnumVariation::Consts which generates enums as global constants)
            .default_enum_style(bindgen::EnumVariation::ModuleConsts)
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            .parse_callbacks(Box::new(WdkCallbacks::new(config)))
            .formatter(bindgen::Formatter::Prettyplease);

        Ok(builder)
    }
}

impl ParseCallbacks for WdkCallbacks {
    fn generated_name_override(&self, item_info: ItemInfo) -> Option<String> {
        // Override the generated name for the WDF function table symbol, since bindgen is unable to currently translate the #define automatically: https://github.com/rust-lang/rust-bindgen/issues/2544
        if let Some(wdf_function_table_symbol_name) = &self.wdf_function_table_symbol_name {
            if let ItemInfo {
                name: item_name,
                kind: ItemKind::Var,
                ..
            } = item_info
            {
                if item_name == wdf_function_table_symbol_name {
                    return Some("WdfFunctions".to_string());
                }
            }
        }
        None
    }

    fn func_macro(&self, name: &str, _value: &[&[u8]]) {
        if name == "DECLARE_HANDLE" {
            println!("{:?}", _value);
        }
    }
}

impl WdkCallbacks {
    fn new(config: &Config) -> Self {
        Self {
            wdf_function_table_symbol_name: config.compute_wdffunctions_symbol_name(),
        }
    }
}
