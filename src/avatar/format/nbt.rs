use std::collections::HashMap;
use std::io::{self, Cursor, Read};

use flate2::read::GzDecoder;

const MAX_NBT_DEPTH: usize = 64;
const MAX_NBT_COLLECTION_LEN: usize = 1_048_576;

#[derive(Clone, Debug, Default)]
pub struct NbtCompound {
    pub entries: HashMap<String, NbtValue>,
}

#[derive(Clone, Debug)]
pub enum NbtValue {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(Vec<NbtValue>),
    Compound(NbtCompound),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

#[repr(u8)]
enum NbtTag {
    End = 0,
    Byte = 1,
    Short = 2,
    Int = 3,
    Long = 4,
    Float = 5,
    Double = 6,
    ByteArray = 7,
    String = 8,
    List = 9,
    Compound = 10,
    IntArray = 11,
    LongArray = 12,
}

pub fn parse_gzip_compound(bytes: &[u8]) -> io::Result<NbtCompound> {
    let decoder = GzDecoder::new(bytes);
    parse_compound(decoder)
}

pub fn parse_compound<R: Read>(mut reader: R) -> io::Result<NbtCompound> {
    let tag = read_u8(&mut reader)?;
    if tag != NbtTag::Compound as u8 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "root tag must be compound",
        ));
    }
    let _name = read_string(&mut reader)?;
    read_compound_payload(&mut reader, 0)
}

fn read_compound_payload<R: Read>(reader: &mut R, depth: usize) -> io::Result<NbtCompound> {
    check_depth(depth)?;
    let mut entries = HashMap::new();
    loop {
        let tag = read_u8(reader)?;
        if tag == NbtTag::End as u8 {
            break;
        }
        let name = read_string(reader)?;
        let value = read_tag_payload(reader, tag, depth + 1)?;
        entries.insert(name, value);
    }
    Ok(NbtCompound { entries })
}

fn read_tag_payload<R: Read>(reader: &mut R, tag: u8, depth: usize) -> io::Result<NbtValue> {
    check_depth(depth)?;
    Ok(match tag {
        x if x == NbtTag::Byte as u8 => NbtValue::Byte(read_i8(reader)?),
        x if x == NbtTag::Short as u8 => NbtValue::Short(read_i16_be(reader)?),
        x if x == NbtTag::Int as u8 => NbtValue::Int(read_i32_be(reader)?),
        x if x == NbtTag::Long as u8 => NbtValue::Long(read_i64_be(reader)?),
        x if x == NbtTag::Float as u8 => NbtValue::Float(read_f32_be(reader)?),
        x if x == NbtTag::Double as u8 => NbtValue::Double(read_f64_be(reader)?),
        x if x == NbtTag::ByteArray as u8 => {
            let len = read_collection_len(reader)?;
            let mut buf = vec![0_i8; len];
            read_exact_i8(reader, &mut buf)?;
            NbtValue::ByteArray(buf)
        }
        x if x == NbtTag::String as u8 => NbtValue::String(read_string(reader)?),
        x if x == NbtTag::List as u8 => {
            let child_tag = read_u8(reader)?;
            let len = read_collection_len(reader)?;
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                items.push(read_tag_payload(reader, child_tag, depth + 1)?);
            }
            NbtValue::List(items)
        }
        x if x == NbtTag::Compound as u8 => {
            NbtValue::Compound(read_compound_payload(reader, depth + 1)?)
        }
        x if x == NbtTag::IntArray as u8 => {
            let len = read_collection_len(reader)?;
            let mut values = Vec::with_capacity(len);
            for _ in 0..len {
                values.push(read_i32_be(reader)?);
            }
            NbtValue::IntArray(values)
        }
        x if x == NbtTag::LongArray as u8 => {
            let len = read_collection_len(reader)?;
            let mut values = Vec::with_capacity(len);
            for _ in 0..len {
                values.push(read_i64_be(reader)?);
            }
            NbtValue::LongArray(values)
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown NBT tag {tag}"),
            ));
        }
    })
}

fn check_depth(depth: usize) -> io::Result<()> {
    if depth > MAX_NBT_DEPTH {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NBT nesting limit exceeded",
        ))
    } else {
        Ok(())
    }
}

fn read_collection_len<R: Read>(reader: &mut R) -> io::Result<usize> {
    let len = read_i32_be(reader)?;
    if len < 0 || len as usize > MAX_NBT_COLLECTION_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid NBT collection length {len}"),
        ));
    }
    Ok(len as usize)
}

fn read_string<R: Read>(reader: &mut R) -> io::Result<String> {
    let len = read_u16_be(reader)? as usize;
    let mut buf = vec![0_u8; len];
    reader.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn read_u8<R: Read>(reader: &mut R) -> io::Result<u8> {
    let mut buf = [0_u8; 1];
    reader.read_exact(&mut buf)?;
    Ok(buf[0])
}

fn read_i8<R: Read>(reader: &mut R) -> io::Result<i8> {
    Ok(read_u8(reader)? as i8)
}

fn read_exact_i8<R: Read>(reader: &mut R, buf: &mut [i8]) -> io::Result<()> {
    let mut bytes = vec![0_u8; buf.len()];
    reader.read_exact(&mut bytes)?;
    for (idx, byte) in bytes.into_iter().enumerate() {
        buf[idx] = byte as i8;
    }
    Ok(())
}

fn read_u16_be<R: Read>(reader: &mut R) -> io::Result<u16> {
    let mut buf = [0_u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(u16::from_be_bytes(buf))
}

fn read_i16_be<R: Read>(reader: &mut R) -> io::Result<i16> {
    Ok(read_u16_be(reader)? as i16)
}

fn read_i32_be<R: Read>(reader: &mut R) -> io::Result<i32> {
    let mut buf = [0_u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(i32::from_be_bytes(buf))
}

fn read_i64_be<R: Read>(reader: &mut R) -> io::Result<i64> {
    let mut buf = [0_u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(i64::from_be_bytes(buf))
}

fn read_f32_be<R: Read>(reader: &mut R) -> io::Result<f32> {
    Ok(f32::from_bits(read_i32_be(reader)? as u32))
}

fn read_f64_be<R: Read>(reader: &mut R) -> io::Result<f64> {
    Ok(f64::from_bits(read_i64_be(reader)? as u64))
}

impl NbtCompound {
    pub fn get_string(&self, key: &str) -> Option<&str> {
        match self.entries.get(key)? {
            NbtValue::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn get_int(&self, key: &str) -> Option<i32> {
        match self.entries.get(key)? {
            NbtValue::Int(value) => Some(*value),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uncompressed_compound() {
        let mut payload = Vec::new();
        write_named_string(&mut payload, "name", "Figura");
        write_named_int(&mut payload, "version", 1);
        payload.push(NbtTag::End as u8);

        let mut root = Vec::new();
        root.push(NbtTag::Compound as u8);
        root.extend_from_slice(&0_u16.to_be_bytes());
        root.extend(payload);

        let compound = parse_compound(Cursor::new(root)).expect("nbt parse");
        assert_eq!(compound.get_string("name"), Some("Figura"));
        assert_eq!(compound.get_int("version"), Some(1));
    }

    #[test]
    fn rejects_negative_collection_length() {
        let mut root = vec![NbtTag::Compound as u8, 0, 0];
        write_named_tag(&mut root, NbtTag::ByteArray as u8, "bytes");
        root.extend_from_slice(&(-1_i32).to_be_bytes());

        let error = parse_compound(Cursor::new(root)).expect_err("negative length must fail");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    fn write_named_string(buf: &mut Vec<u8>, name: &str, value: &str) {
        write_named_tag(buf, NbtTag::String as u8, name);
        let bytes = value.as_bytes();
        buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(bytes);
    }

    fn write_named_int(buf: &mut Vec<u8>, name: &str, value: i32) {
        write_named_tag(buf, NbtTag::Int as u8, name);
        buf.extend_from_slice(&value.to_be_bytes());
    }

    fn write_named_tag(buf: &mut Vec<u8>, tag: u8, name: &str) {
        buf.push(tag);
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(name_bytes);
    }
}
