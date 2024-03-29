use crate::{
    buffer_pool::{AutoReturnBufferDeserializer, BufferPool},
    protocol::KvsRequest,
};
use serde::de::{SeqAccess, Visitor};
use std::{cell::RefCell, rc::Rc};

pub struct KvsRequestPutVisitor {
    pub collector: Rc<RefCell<BufferPool>>,
}
impl<'de> Visitor<'de> for KvsRequestPutVisitor {
    type Value = KvsRequest;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("KvsRequest::Put")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let key: u64 = seq.next_element()?.unwrap();
        let value = seq
            .next_element_seed(AutoReturnBufferDeserializer {
                collector: self.collector,
            })?
            .unwrap();

        Ok(KvsRequest::Put { key, value })
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut key = None;
        let mut buffer = None;

        loop {
            let k: Option<String> = map.next_key()?;
            if let Some(k) = k {
                match k.as_str() {
                    "key" => {
                        key = Some(map.next_value()?);
                    }
                    "value" => {
                        buffer = Some(map.next_value_seed(AutoReturnBufferDeserializer {
                            collector: self.collector.clone(),
                        })?);
                    }
                    _ => panic!(),
                }
            } else {
                break;
            }
        }

        assert!(key.is_some());
        assert!(buffer.is_some());

        Ok(KvsRequest::Put {
            key: key.unwrap(),
            value: buffer.unwrap(),
        })
    }
}
