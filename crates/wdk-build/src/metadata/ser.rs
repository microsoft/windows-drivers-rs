// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use serde::{
    ser::{self, Impossible},
    Serialize,
};

use super::{
    error::{Error, Result},
    map::Map,
};

/// delimiter used to separate the names of the different nodes encoded into a
/// key name. Since `-` is not valid in Rust identifiers, it is used
/// as a separator between different node names.
pub const KEY_NAME_SEPARATOR: char = '-';

/// delimiter used to separate the different elemets in a sequence.
/// Sequence elements are serialized into a single string with this delimiter.
pub const SEQ_ELEMENT_SEPARATOR: char = ';';

/// Serialize a value into a [`Map`] where the keys represent a
/// `KEY_NAME_SEPARATOR`-seperated list of field names.
///
/// # Errors
///
/// This function will return an error if the type being serialized:
/// * results in duplicate key names
/// * results in an empty key name
/// * otherwise fails to be parsed and correctly serialized into a [`Map`]
///
/// # Example
/// ```rust
/// use std::collections::BTreeMap;
///
/// use wdk_build::metadata::{
///     self,
///     driver_settings::{DriverConfig, KmdfConfig},
///     to_map,
/// };
///
/// let wdk_metadata = metadata::Wdk {
///     driver_model: DriverConfig::Kmdf(KmdfConfig {
///         kmdf_version_major: 1,
///         target_kmdf_version_minor: 23,
///         minimum_kmdf_version_minor: None,
///     }),
///     driver_install: None,
/// };
///
/// let output = to_map::<BTreeMap<_, _>>(&wdk_metadata).unwrap();
///
/// assert_eq!(output["DRIVER_MODEL-DRIVER_TYPE"], "KMDF");
/// assert_eq!(output["DRIVER_MODEL-KMDF_VERSION_MAJOR"], "1");
/// assert_eq!(output["DRIVER_MODEL-TARGET_KMDF_VERSION_MINOR"], "23");
///
/// // `None` values are not serialized
/// assert_eq!(output.get("DRIVER_MODEL-MINIMUM_KMDF_VERSION_MINOR"), None);
/// ```
pub fn to_map<M>(value: &impl Serialize) -> Result<M>
where
    M: Map<String, String>,
{
    let mut serialization_buffer: Vec<(String, String)> = Vec::new();
    value.serialize(&mut Serializer::new(&mut serialization_buffer))?;
    convert_serialized_output_to_map(serialization_buffer)
}

/// Serialize a value into a [`Map`] where the keys represent a
/// `KEY_NAME_SEPARATOR`-seperated list of field names prepended with a
/// prefix.
///
/// # Errors
///
/// This function will return an error if the type being serialized:
/// * results in duplicate key names
/// * results in an empty key name
/// * otherwise fails to be parsed and correctly serialized into a [`Map`]
///
/// # Example
/// ```rust
/// use std::collections::BTreeMap;
///
/// use wdk_build::metadata::{
///     self,
///     driver_settings::{DriverConfig, KmdfConfig},
///     to_map_with_prefix,
/// };
///
/// let wdk_metadata = metadata::Wdk {
///     driver_model: DriverConfig::Kmdf(KmdfConfig {
///         kmdf_version_major: 1,
///         target_kmdf_version_minor: 33,
///         minimum_kmdf_version_minor: Some(31),
///     }),
///     driver_install: None,
/// };
///
/// let output = to_map_with_prefix::<BTreeMap<_, _>>("WDK_BUILD_METADATA", &wdk_metadata).unwrap();
///
/// assert_eq!(
///     output["WDK_BUILD_METADATA-DRIVER_MODEL-DRIVER_TYPE"],
///     "KMDF"
/// );
/// assert_eq!(
///     output["WDK_BUILD_METADATA-DRIVER_MODEL-KMDF_VERSION_MAJOR"],
///     "1"
/// );
/// assert_eq!(
///     output["WDK_BUILD_METADATA-DRIVER_MODEL-TARGET_KMDF_VERSION_MINOR"],
///     "33"
/// );
/// assert_eq!(
///     output["WDK_BUILD_METADATA-DRIVER_MODEL-MINIMUM_KMDF_VERSION_MINOR"],
///     "31"
/// );
/// ```
pub fn to_map_with_prefix<M>(prefix: impl Into<String>, value: &impl Serialize) -> Result<M>
where
    M: Map<String, String>,
{
    let mut serialization_buffer: Vec<(String, String)> = Vec::new();
    value.serialize(&mut Serializer::with_prefix(
        prefix.into(),
        &mut serialization_buffer,
    ))?;
    convert_serialized_output_to_map(serialization_buffer)
}

fn convert_serialized_output_to_map<M>(serialization_buffer: Vec<(String, String)>) -> Result<M>
where
    M: Map<String, String>,
{
    let mut output_map = M::new();
    for (key, value) in serialization_buffer {
        output_map.insert_or_else(key, value, |key, existing_value, new_value| {
            Err(Error::DuplicateSerializationKeys {
                key: key.clone(),
                value_1: existing_value.clone(),
                value_2: new_value,
            })
        })?;
    }

    Ok(output_map)
}

/// [`serde`] serializer that serializes values into a [`Vec`] of key-value
/// pairs.
///
/// This serializer is useful when you want to have more granular control of the
/// output of the serializer. Most usecases should already be covered by the
/// [`to_map`] and [`to_map_with_prefix`] functions.
pub struct Serializer<'a> {
    root_key_name: Option<String>,
    dst: &'a mut Vec<(String, String)>,
}

impl<'a> ser::Serializer for &'a mut Serializer<'a> {
    type Error = Error;
    type Ok = Option<String>;
    type SerializeMap = Impossible<Self::Ok, Self::Error>;
    type SerializeSeq = SerializerSeq<'a>;
    type SerializeStruct = Self;
    type SerializeStructVariant = Impossible<Self::Ok, Self::Error>;
    type SerializeTuple = Impossible<Self::Ok, Self::Error>;
    type SerializeTupleStruct = Impossible<Self::Ok, Self::Error>;
    type SerializeTupleVariant = Impossible<Self::Ok, Self::Error>;

    unsupported_serde_serialize_method! {
        // simple types
        bytes newtype_struct newtype_variant unit_struct unit_variant
        // complex types (returns SerializeXYZ types)
        map struct_variant tuple tuple_struct tuple_variant
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_bool(self, value: bool) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_f32(self, value: f32) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_f64(self, value: f64) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_none(self) -> Result<Self::Ok> {
        self.serialize_unit()
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok> {
        Ok(None)
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok> {
        Ok(Some(value.to_string()))
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Ok(self)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        Ok(SerializerSeq::new(self.root_key_name.clone(), self.dst))
    }
}

impl<'a> ser::SerializeStruct for &'a mut Serializer<'a> {
    type Error = Error;
    type Ok = Option<String>;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        let new_root_key_name = self.root_key_name.clone().map_or_else(
            || key.to_string(),
            |root_key_name| format!("{root_key_name}{KEY_NAME_SEPARATOR}{key}"),
        );

        let mut serializer = Serializer::with_prefix(new_root_key_name.clone(), self.dst);
        let value_string = value.serialize(&mut serializer)?;

        if let Some(value_string) = value_string {
            self.dst.push((new_root_key_name, value_string));
        }

        Ok(())
    }

    fn end(self) -> Result<Self::Ok> {
        Ok(None)
    }
}

pub struct SerializerSeq<'a> {
    root_key_name: Option<String>,
    dst: &'a mut Vec<(String, String)>,
    delimited_string: String,
}

impl ser::SerializeSeq for SerializerSeq<'_> {
    type Error = Error;
    type Ok = Option<String>;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        let root_key_name = self.root_key_name.clone().unwrap();
        let mut serializer = Serializer::with_prefix(root_key_name, self.dst);
        let value_string = value.serialize(&mut serializer)?.unwrap_or_else(|| {
            unimplemented!(
                "Sequence serializer for elements of type {} is not suppoted",
                std::any::type_name::<T>(),
            )
        });

        self.delimited_string.push_str(&value_string);
        self.delimited_string.push(SEQ_ELEMENT_SEPARATOR);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok> {
        // Remove the trailing delimiter.
        let delimited_string = if self.delimited_string.is_empty() {
            String::new()
        } else {
            self.delimited_string[..self.delimited_string.len() - 1].to_string()
        };

        self.dst
            .push((self.root_key_name.unwrap(), delimited_string));
        Ok(None)
    }
}

impl<'a> Serializer<'a> {
    /// Create a new instance of the `Serializer` struct
    pub fn new(dst: &'a mut Vec<(String, String)>) -> Self {
        Self {
            root_key_name: None,
            dst,
        }
    }

    /// Create a new instance of the `Serializer` struct with a prefix used as
    /// the root for all keys
    pub fn with_prefix(prefix: String, dst: &'a mut Vec<(String, String)>) -> Self {
        Self {
            root_key_name: Some(prefix),
            dst,
        }
    }
}

impl<'a> SerializerSeq<'a> {
    /// Create a new instance of the `SerializerSeq` struct
    pub fn new(root_key_name: Option<String>, dst: &'a mut Vec<(String, String)>) -> Self {
        Self {
            root_key_name,
            dst,
            delimited_string: String::new(),
        }
    }
}

#[doc(hidden)]
/// Helper macro when implementing the `Serializer` part of a new data
/// format for Serde.
///
/// Generates [`serde::ser::Serializer`] trait methods for serde data model
/// types that aren't supported by this serializer. This generates a
/// method that calls [`unimplemented!`].
macro_rules! unsupported_serde_serialize_method {
    ($($method_type:ident)*) => {
        $(unsupported_serde_serialize_method_helper! {$method_type})*
    };
}
#[doc(hidden)]
pub(crate) use unsupported_serde_serialize_method;

#[doc(hidden)]
macro_rules! unsupported_serde_serialize_method_helper {
    // methods for simple types (returns Ok)
    (bytes) => {
        unsupported_serde_serialize_method_definition! {
            serialize_bytes(_v: &[u8]) -> std::result::Result<
                <Self as serde::ser::Serializer>::Ok,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (newtype_struct) => {
        unsupported_serde_serialize_method_definition! {
            serialize_newtype_struct<T>(_name: &'static str, _value: &T) -> std::result::Result<
                <Self as serde::ser::Serializer>::Ok,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (newtype_variant) => {
        unsupported_serde_serialize_method_definition! {
            serialize_newtype_variant<T>(_name: &'static str, _variant_index: u32, _variant: &'static str, _value: &T) -> std::result::Result<
                <Self as serde::ser::Serializer>::Ok,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (none) => {
        unsupported_serde_serialize_method_definition! {
            serialize_none() -> std::result::Result<
                <Self as serde::ser::Serializer>::Ok,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (some) => {
        unsupported_serde_serialize_method_definition! {
            serialize_some<T>(_value: &T) -> std::result::Result<
                <Self as serde::ser::Serializer>::Ok,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (str) => {
        unsupported_serde_serialize_method_definition! {
            serialize_str(_v: &str) -> std::result::Result<
                <Self as serde::ser::Serializer>::Ok,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (unit) => {
        unsupported_serde_serialize_method_definition! {
            serialize_unit() -> std::result::Result<
                <Self as serde::ser::Serializer>::Ok,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (unit_struct) => {
        unsupported_serde_serialize_method_definition! {
            serialize_unit_struct(_name: &'static str) -> std::result::Result<
                <Self as serde::ser::Serializer>::Ok,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (unit_variant) => {
        unsupported_serde_serialize_method_definition! {
            serialize_unit_variant(_name: &'static str, _variant_index: u32, _variant: &'static str) -> std::result::Result<
                <Self as serde::ser::Serializer>::Ok,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    // methods for complex types (returns SerializeXYZ types)
    (map) => {
        unsupported_serde_serialize_method_definition! {
            serialize_map(_len: Option<usize>) -> std::result::Result<
                <Self as serde::ser::Serializer>::SerializeMap,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (struct) => {
        unsupported_serde_serialize_method_definition! {
            serialize_struct(_name: &'static str, _len: usize) -> std::result::Result<
                <Self as serde::ser::Serializer>::SerializeStruct,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (struct_variant) => {
        unsupported_serde_serialize_method_definition! {
            serialize_struct_variant(_name: &'static str, _variant_index: u32, _variant: &'static str, _len: usize) -> std::result::Result<
                <Self as serde::ser::Serializer>::SerializeStructVariant,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (seq) => {
        unsupported_serde_serialize_method_definition! {
            serialize_seq(_len: Option<usize>) -> std::result::Result<
                <Self as serde::ser::Serializer>::SerializeSeq,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (tuple) => {
        unsupported_serde_serialize_method_definition! {
            serialize_tuple(_len: usize) -> std::result::Result<
                <Self as serde::ser::Serializer>::SerializeTuple,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (tuple_struct) => {
        unsupported_serde_serialize_method_definition! {
            serialize_tuple_struct(_name: &'static str, _len: usize) -> std::result::Result<
                <Self as serde::ser::Serializer>::SerializeTupleStruct,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    (tuple_variant) => {
        unsupported_serde_serialize_method_definition! {
            serialize_tuple_variant(_name: &'static str, _variant_index: u32, _variant: &'static str, _len: usize) -> std::result::Result<
                <Self as serde::ser::Serializer>::SerializeTupleVariant,
                <Self as serde::ser::Serializer>::Error,
            >
        }
    };
    // every other method has no extra arguments and is for simple types
    ($method_type:ident) => {
        paste::paste! {
            unsupported_serde_serialize_method_definition! {
                [<serialize_ $method_type>](_v: $method_type) -> std::result::Result<
                    <Self as serde::ser::Serializer>::Ok,
                    <Self as serde::ser::Serializer>::Error,
                >
            }
        }
    };
}
#[doc(hidden)]
pub(crate) use unsupported_serde_serialize_method_helper;

#[doc(hidden)]
macro_rules! unsupported_serde_serialize_method_definition {
    // methods with generic argument
    ($func:ident <$generic_arg:ident> ($($arg:ident : $ty:ty),*) -> std::result::Result<$ok:ty, $err:ty$(,)?>) => {
        #[inline]
        fn $func <$generic_arg> (self, $($arg: $ty,)*) -> std::result::Result<$ok, $err>
        where
        $generic_arg: ?Sized + Serialize {
            unimplemented!(
                "{} is not implemented for {} since it is currently not needed to serialize the metadata::Wdk struct",
                stringify!($func),
                std::any::type_name::<Self>(),
            )
        }
    };
    // methods without generic argument
    ($func:ident ($($arg:ident : $ty:ty),*) -> std::result::Result<$ok:ty, $err:ty$(,)?>) => {
        #[inline]
        fn $func (self, $($arg: $ty,)*) -> std::result::Result<$ok, $err> {
            unimplemented!(
                "{} is not implemented for {} since it is currently not needed to serialize the metadata::Wdk struct",
                stringify!($func),
                std::any::type_name::<Self>(),
            )
        }
    };
}
#[doc(hidden)]
pub(crate) use unsupported_serde_serialize_method_definition;

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        vec,
    };

    use super::*;
    use crate::metadata::{
        self,
        driver_install::DriverInstall,
        driver_settings::{DriverConfig, KmdfConfig, UmdfConfig},
    };

    #[test]
    fn test_kmdf() {
        let wdk_metadata = metadata::Wdk {
            driver_model: DriverConfig::Kmdf(KmdfConfig {
                kmdf_version_major: 1,
                target_kmdf_version_minor: 23,
                minimum_kmdf_version_minor: Some(21),
            }),
            driver_install: None,
        };

        let output = to_map::<BTreeMap<_, _>>(&wdk_metadata).unwrap();

        assert_eq!(output["DRIVER_MODEL-DRIVER_TYPE"], "KMDF");
        assert_eq!(output["DRIVER_MODEL-KMDF_VERSION_MAJOR"], "1");
        assert_eq!(output["DRIVER_MODEL-TARGET_KMDF_VERSION_MINOR"], "23");
        assert_eq!(output["DRIVER_MODEL-MINIMUM_KMDF_VERSION_MINOR"], "21");
    }

    #[test]
    fn test_kmdf_no_minimum() {
        let wdk_metadata = metadata::Wdk {
            driver_model: DriverConfig::Kmdf(KmdfConfig {
                kmdf_version_major: 1,
                target_kmdf_version_minor: 23,
                minimum_kmdf_version_minor: None,
            }),
            driver_install: None,
        };

        let output = to_map::<BTreeMap<_, _>>(&wdk_metadata).unwrap();

        assert_eq!(output["DRIVER_MODEL-DRIVER_TYPE"], "KMDF");
        assert_eq!(output["DRIVER_MODEL-KMDF_VERSION_MAJOR"], "1");
        assert_eq!(output["DRIVER_MODEL-TARGET_KMDF_VERSION_MINOR"], "23");

        // `None` values are not serialized
        assert_eq!(output.get("DRIVER_MODEL-MINIMUM_KMDF_VERSION_MINOR"), None);
    }

    #[test]
    fn test_kmdf_with_prefix() {
        let wdk_metadata = metadata::Wdk {
            driver_model: DriverConfig::Kmdf(KmdfConfig {
                kmdf_version_major: 1,
                target_kmdf_version_minor: 33,
                minimum_kmdf_version_minor: Some(31),
            }),
            driver_install: None,
        };

        let output =
            to_map_with_prefix::<BTreeMap<_, _>>("WDK_BUILD_METADATA", &wdk_metadata).unwrap();

        assert_eq!(
            output["WDK_BUILD_METADATA-DRIVER_MODEL-DRIVER_TYPE"],
            "KMDF"
        );
        assert_eq!(
            output["WDK_BUILD_METADATA-DRIVER_MODEL-KMDF_VERSION_MAJOR"],
            "1"
        );
        assert_eq!(
            output["WDK_BUILD_METADATA-DRIVER_MODEL-TARGET_KMDF_VERSION_MINOR"],
            "33"
        );
        assert_eq!(
            output["WDK_BUILD_METADATA-DRIVER_MODEL-MINIMUM_KMDF_VERSION_MINOR"],
            "31"
        );
    }

    #[test]
    fn test_kmdf_with_hashmap() {
        let wdk_metadata = metadata::Wdk {
            driver_model: DriverConfig::Kmdf(KmdfConfig {
                kmdf_version_major: 1,
                target_kmdf_version_minor: 33,
                minimum_kmdf_version_minor: Some(31),
            }),
            driver_install: None,
        };

        let output = to_map::<HashMap<_, _>>(&wdk_metadata).unwrap();

        assert_eq!(output["DRIVER_MODEL-DRIVER_TYPE"], "KMDF");
        assert_eq!(output["DRIVER_MODEL-KMDF_VERSION_MAJOR"], "1");
        assert_eq!(output["DRIVER_MODEL-TARGET_KMDF_VERSION_MINOR"], "33");
        assert_eq!(output["DRIVER_MODEL-MINIMUM_KMDF_VERSION_MINOR"], "31");
    }

    #[test]
    fn test_umdf() {
        let wdk_metadata = metadata::Wdk {
            driver_model: DriverConfig::Umdf(UmdfConfig {
                umdf_version_major: 1,
                target_umdf_version_minor: 23,
                minimum_umdf_version_minor: Some(21),
            }),
            driver_install: None,
        };

        let output = to_map::<BTreeMap<_, _>>(&wdk_metadata).unwrap();

        assert_eq!(output["DRIVER_MODEL-DRIVER_TYPE"], "UMDF");
        assert_eq!(output["DRIVER_MODEL-UMDF_VERSION_MAJOR"], "1");
        assert_eq!(output["DRIVER_MODEL-TARGET_UMDF_VERSION_MINOR"], "23");
        assert_eq!(output["DRIVER_MODEL-MINIMUM_UMDF_VERSION_MINOR"], "21");
    }

    #[test]
    fn test_umdf_no_minimum() {
        let wdk_metadata = metadata::Wdk {
            driver_model: DriverConfig::Umdf(UmdfConfig {
                umdf_version_major: 1,
                target_umdf_version_minor: 23,
                minimum_umdf_version_minor: None,
            }),
            driver_install: None,
        };

        let output = to_map::<BTreeMap<_, _>>(&wdk_metadata).unwrap();

        assert_eq!(output["DRIVER_MODEL-DRIVER_TYPE"], "UMDF");
        assert_eq!(output["DRIVER_MODEL-UMDF_VERSION_MAJOR"], "1");
        assert_eq!(output["DRIVER_MODEL-TARGET_UMDF_VERSION_MINOR"], "23");

        // `None` values are not serialized
        assert_eq!(output.get("DRIVER_MODEL-MINIMUM_UMDF_VERSION_MINOR"), None);
    }

    #[test]
    fn test_wdm() {
        let wdk_metadata = metadata::Wdk {
            driver_model: DriverConfig::Wdm,
            driver_install: None,
        };

        let output = to_map::<BTreeMap<_, _>>(&wdk_metadata).unwrap();

        assert_eq!(output["DRIVER_MODEL-DRIVER_TYPE"], "WDM");
    }

    #[test]
    fn test_conflicting_keys_in_convert_serialized_output_to_map() {
        let input = vec![("KEY_NAME", "VALUE_1"), ("KEY_NAME", "VALUE_2")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let err = convert_serialized_output_to_map::<BTreeMap<_, _>>(input).unwrap_err();

        assert!(matches!(
            err,
            Error::DuplicateSerializationKeys {
                key,
                value_1,
                value_2,
            } if key == "KEY_NAME" && value_1 == "VALUE_1" && value_2 == "VALUE_2"
        ));
    }

    #[test]
    fn test_package_files() {
        let package_files = vec![
            "file1.exe".to_string(),
            "file2.sys".to_string(),
            "file3.dll".to_string(),
        ];

        let wdk_metadata = metadata::Wdk {
            driver_model: DriverConfig::Wdm,
            driver_install: Some(DriverInstall {
                package_files: package_files.clone(),
            }),
        };

        let output = to_map::<BTreeMap<_, _>>(&wdk_metadata).unwrap();

        assert_eq!(output["DRIVER_MODEL-DRIVER_TYPE"], "WDM");
        assert_eq!(
            output["DRIVER_INSTALL-PACKAGE_FILES"],
            package_files.join(";")
        );
    }

    #[test]
    fn test_diverse_package_files() {
        let package_files = vec![
            "typical.exe".to_string(),
            "with whitespace.sys".to_string(),
            "underscored_file_.dll".to_string(),
            "noextension".to_string(),
        ];

        let wdk_metadata = metadata::Wdk {
            driver_model: DriverConfig::Wdm,
            driver_install: Some(DriverInstall {
                package_files: package_files.clone(),
            }),
        };

        let output = to_map::<BTreeMap<_, _>>(&wdk_metadata).unwrap();

        assert_eq!(output["DRIVER_MODEL-DRIVER_TYPE"], "WDM");
        assert_eq!(
            output["DRIVER_INSTALL-PACKAGE_FILES"],
            package_files.join(";")
        );
    }

    #[test]
    fn test_empty_package_files() {
        let wdk_metadata = metadata::Wdk {
            driver_model: DriverConfig::Wdm,
            driver_install: Some(DriverInstall {
                package_files: vec![],
            }),
        };

        let output = to_map::<BTreeMap<_, _>>(&wdk_metadata).unwrap();

        assert_eq!(output["DRIVER_MODEL-DRIVER_TYPE"], "WDM");
        assert_eq!(output["DRIVER_INSTALL-PACKAGE_FILES"], "");
    }

    #[derive(Serialize)]
    struct UnsupportedSequenceStruct {
        field1: u8,
        field2: u8,
    }
    #[derive(Serialize)]
    struct UnsupportedSequenceParentStruct {
        sequence: Vec<UnsupportedSequenceStruct>,
    }
    #[test]
    #[should_panic(
        expected = "not implemented: Sequence serializer for elements of type \
                    &wdk_build::metadata::ser::tests::UnsupportedSequenceStruct is not suppoted"
    )]
    fn test_unsupported_seq() {
        let unsuppoted_struct = UnsupportedSequenceParentStruct {
            sequence: vec![
                UnsupportedSequenceStruct {
                    field1: 1,
                    field2: 2,
                },
                UnsupportedSequenceStruct {
                    field1: 3,
                    field2: 4,
                },
            ],
        };

        let _output = to_map::<BTreeMap<_, _>>(&unsuppoted_struct).unwrap();
    }
}
