#![no_main]
#![deny(warnings)]
const GUID_DEVINTERFACE_COMPORT: wdk_sys::GUID = wdk_sys::GUID {
    Data1: 0x86E0D1E0u32,
    Data2: 0x8089u16,
    Data3: 0x11D0u16,
    Data4: [0x9Cu8, 0xE4u8, 0x08u8, 0x00u8, 0x3Eu8, 0x30u8, 0x1Fu8, 0x73u8],
};
fn create_device_interface(wdf_device: wdk_sys::WDFDEVICE) -> wdk_sys::NTSTATUS {
    unsafe {
        {
            use wdk_sys::*;
            #[must_use]
            #[inline(always)]
            #[allow(non_snake_case)]
            unsafe fn wdf_device_create_device_interface_impl(
                Device: WDFDEVICE,
                InterfaceClassGUID: *const GUID,
                ReferenceString: PCUNICODE_STRING,
            ) -> NTSTATUS {
                let wdf_function: wdk_sys::PFN_WDFDEVICECREATEDEVICEINTERFACE = Some(unsafe {
                    core::mem::transmute(
                        wdk_sys::WDF_FUNCTION_TABLE[wdk_sys::_WDFFUNCENUM::WdfDeviceCreateDeviceInterfaceTableIndex
                            as usize],
                    )
                });
                if let Some(wdf_function) = wdf_function {
                    unsafe {
                        (wdf_function)(
                            wdk_sys::WdfDriverGlobals,
                            Device,
                            InterfaceClassGUID,
                            ReferenceString,
                        )
                    }
                } else {
                    {
                        ::core::panicking::panic_fmt(
                            format_args!(
                                "internal error: entered unreachable code: {0}",
                                format_args!("Option should never be None"),
                            ),
                        );
                    };
                }
            }
            wdf_device_create_device_interface_impl(
                wdf_device,
                &GUID_DEVINTERFACE_COMPORT,
                core::ptr::null(),
            )
        }
    }
}
