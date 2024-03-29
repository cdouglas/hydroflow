mod kvs_request_delete_visitor;
mod kvs_request_get_visitor;
mod kvs_request_gossip_visitor;
mod kvs_request_put_visitor;

use super::KvsRequest;
use crate::{
    buffer_pool::BufferPool,
    protocol::serialization::{
        kvs_request_delete_visitor::KvsRequestDeleteVisitor,
        kvs_request_get_visitor::KvsRequestGetVisitor,
        kvs_request_gossip_visitor::KvsRequestGossipVisitor,
        kvs_request_put_visitor::KvsRequestPutVisitor,
    },
};
use serde::{
    de::{DeserializeSeed, VariantAccess, Visitor},
    ser::SerializeStructVariant,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{cell::RefCell, rc::Rc};

impl Serialize for KvsRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            KvsRequest::Put { key, value } => {
                let mut s = serializer.serialize_struct_variant("KvsRequest", 0, "Put", 2)?;
                s.serialize_field("key", key)?;
                s.serialize_field("value", value)?;
                s.end()
            }
            KvsRequest::Get { key } => {
                let mut s = serializer.serialize_struct_variant("KvsRequest", 1, "Get", 1)?;
                s.serialize_field("key", key)?;
                s.end()
            }
            KvsRequest::Gossip { key, reg } => {
                let mut s = serializer.serialize_struct_variant("KvsRequest", 2, "Gossip", 3)?;
                s.serialize_field("key", key)?;
                s.serialize_field("marker", &reg.key.0)?;
                s.serialize_field("buffer", &reg.val.0.as_ref().map(|x| &x.0))?; // Serialize Option<AutoReturnBuffer> to minimze amount of boilerplate to write.
                s.end()
            }
            KvsRequest::Delete { key } => {
                let mut s = serializer.serialize_struct_variant("KvsRequest", 3, "Delete", 1)?;
                s.serialize_field("key", key)?;
                s.end()
            }
        }
    }
}

pub struct KvsRequestDeserializer {
    pub collector: Rc<RefCell<BufferPool>>,
}

impl<'de> DeserializeSeed<'de> for KvsRequestDeserializer {
    type Value = KvsRequest;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct KvsRequestVisitor {
            collector: Rc<RefCell<BufferPool>>,
        }
        impl<'de> Visitor<'de> for KvsRequestVisitor {
            type Value = KvsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("KvsRequest enum")
            }

            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::EnumAccess<'de>,
            {
                Ok(match data.variant()? {
                    (KvsRequestField::Put, variant) => variant.struct_variant(
                        &["key", "value"],
                        KvsRequestPutVisitor {
                            collector: self.collector,
                        },
                    )?,

                    (KvsRequestField::Get, variant) => {
                        variant.struct_variant(&["key"], KvsRequestGetVisitor)?
                    }
                    (KvsRequestField::Gossip, variant) => variant.struct_variant(
                        &["key", "marker", "buffer"],
                        KvsRequestGossipVisitor {
                            collector: self.collector,
                        },
                    )?,
                    (KvsRequestField::Delete, variant) => {
                        variant.struct_variant(&["key"], KvsRequestDeleteVisitor)?
                    }
                })
            }
        }

        deserializer.deserialize_enum(
            "KvsRequest",
            &["Put", "Get", "Gossip", "Delete"],
            KvsRequestVisitor {
                collector: self.collector,
            },
        )
    }
}

enum KvsRequestField {
    Put,
    Get,
    Gossip,
    Delete,
}
struct KVSRequestFieldVisitor;
impl<'de> Visitor<'de> for KVSRequestFieldVisitor {
    type Value = KvsRequestField;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("field identifier")
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match value {
            0 => Result::Ok(KvsRequestField::Put),
            1 => Result::Ok(KvsRequestField::Get),
            2 => Result::Ok(KvsRequestField::Gossip),
            3 => Result::Ok(KvsRequestField::Delete),
            _ => panic!(),
        }
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(match v {
            "Put" => KvsRequestField::Put,
            "Get" => KvsRequestField::Get,
            "Gossip" => KvsRequestField::Gossip,
            "Delete" => KvsRequestField::Delete,
            _ => panic!(),
        })
    }
}
impl<'de> Deserialize<'de> for KvsRequestField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Deserializer::deserialize_identifier(deserializer, KVSRequestFieldVisitor)
    }
}
