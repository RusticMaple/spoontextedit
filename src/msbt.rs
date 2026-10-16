use std::cmp::Ordering;
use std::error::Error;
use std::mem::{offset_of, size_of};
use std::string::FromUtf8Error;
use bytemuck::cast_slice;
use bytemuck::checked::try_cast_slice;
use thiserror::Error;
use crate::msbt;
use crate::msbt::file_repr::{Attribute1Header, Header, Label1Header, LabelOffsetTableNode, Text2Header};
use crate::msbt::LoadError::{InvalidAttribute1Header, InvalidHeader, InvalidLabel1Header, InvalidText2Header, LabelRead, InvalidTableElement, TableElementNotFound, UnableToFindHeader, UnableToFindTable, NotEnoughTexts, TextCommand};
use crate::msbt::TextCommandError::{CutOff, InvalidOrUnimplemented, InvalidTextCommand, NoEnd};

pub mod file_repr {
    use bytemuck::{Pod, Zeroable};
    use thiserror::Error;
    use crate::msbt::file_repr::HeaderValidationError::{InvalidBom, InvalidMagic};
    use crate::msbt::LoadError;

    pub const BIG_ENDIAN: [u8; 2] = [0xFE, 0xFF];
    pub const LITTLE_ENDIAN: [u8; 2] = [0xFF, 0xFE];
    macro_rules! valid_values {
        (($var:expr) != {$($val:path),*} => $err:expr ) => {
            if !($( $var == $val || )* false) {
                return Err($err);
            }
        };
    }

    macro_rules! byteswap {
        ($( $val:ident ),*) => {
            pub(crate) fn byteswap(&mut self, bom: [u8;2]){
                if  bom == BIG_ENDIAN && cfg!(target_endian = "little") ||
                    bom == LITTLE_ENDIAN && cfg!(target_endian = "big"){
                    $(
                    self.$val = self.$val.swap_bytes();
                    )*
                }
            }
        };
    }

    #[repr(C, packed)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    pub struct Version(u8, u8);

    #[repr(C, packed)]
    #[derive(Copy, Clone, Pod, Zeroable)]
    pub(crate) struct Header {
        magic: [u8; 8],
        pub bom: [u8; 2],
        unknown1: u16,

        pub version: Version,
        pub sections: u16,
        unknown2: u16,
        pub file_size: u32,

        unknown3: u16,
        unknown4: u16,
        unknown5: u16,
        unknown6: u16,
        unknown7: u16,
    }


    #[derive(Error, Debug)]
    pub(crate) enum HeaderValidationError {
        #[error("Invalid Bom")]
        InvalidBom,
        #[error("Invalid Magic, expected {0:X?} but got {1:X?}")]
        InvalidMagic(&'static [u8], Box<[u8]>),
    }

    impl Header {
        const MAGIC: [u8; 8] = [
            'M' as u8,
            's' as u8,
            'g' as u8,
            'S' as u8,
            't' as u8,
            'd' as u8,
            'B' as u8,
            'n' as u8];//MsgStdBn


        pub fn check_and_correct(&mut self) -> Result<(), HeaderValidationError> {
            valid_values!((self.magic) != { Self::MAGIC } => InvalidMagic(&Self::MAGIC, self.magic.into()));
            valid_values!((self.bom) != { BIG_ENDIAN, LITTLE_ENDIAN } => InvalidBom);


            self.byteswap(self.bom);

            Ok(())
        }

        byteswap!(file_size,sections);
    }

    #[repr(C)]
    #[derive(Copy, Clone, Pod, Zeroable)]
    pub(crate) struct Label1Header {
        magic: [u8; 4],
        pub table_size: u32,
        padding: u64,
    }


    impl Label1Header {
        const MAGIC: [u8; 4] = [
            'L' as u8,
            'B' as u8,
            'L' as u8,
            '1' as u8, ];

        pub fn check_and_correct(&mut self, bom: [u8; 2]) -> Result<(), HeaderValidationError> {
            valid_values!((self.magic) != {Self::MAGIC} => InvalidMagic(&Self::MAGIC, self.magic.into()));

            self.byteswap(bom);

            Ok(())
        }

        byteswap!(table_size);
    }

    #[repr(C)]
    #[derive(Copy, Clone, Pod, Zeroable)]
    pub(crate) struct LabelOffsetTableNode {
        pub count: u32,
        pub offset: u32,
    }

    impl LabelOffsetTableNode {
        pub fn check_and_correct(&mut self, bom: [u8; 2]) -> Result<(), HeaderValidationError> {
            self.byteswap(bom);

            Ok(())
        }
        byteswap!(count,offset);
    }

    #[repr(C)]
    #[derive(Copy, Clone, Pod, Zeroable)]
    pub(crate) struct Attribute1Header {
        magic: [u8; 4],
        pub table_size: u32,
        _padding: u64,
    }

    impl Attribute1Header {
        const MAGIC: [u8; 4] = [
            'A' as u8,
            'T' as u8,
            'R' as u8,
            '1' as u8, ];

        pub fn check_and_correct(&mut self, bom: [u8; 2]) -> Result<(), HeaderValidationError> {
            self.byteswap(bom);

            valid_values!((self.magic) != {Self::MAGIC} => InvalidMagic(&Self::MAGIC, self.magic.into()));

            Ok(())
        }

        byteswap!(table_size);
    }

    #[repr(C)]
    #[derive(Copy, Clone, Pod, Zeroable)]
    pub(crate) struct Text2Header {
        pub(crate) magic: [u8; 4],
        pub(crate) table_size: u32,
        pub(crate) _padding: u64,
    }

    impl Text2Header {
        pub(crate) const MAGIC: [u8; 4] = [
            'T' as u8,
            'X' as u8,
            'T' as u8,
            '2' as u8, ];

        pub fn check_and_correct(&mut self, bom: [u8; 2]) -> Result<(), HeaderValidationError> {
            self.byteswap(bom);

            valid_values!((self.magic) != {Self::MAGIC} => InvalidMagic(&Self::MAGIC, self.magic.into()));

            Ok(())
        }

        byteswap!(table_size);
    }

    #[derive(Copy, Clone, Pod, Zeroable)]
    #[repr(C, packed(1))]
    pub struct PictureData{
        pub(crate) unk_1: i16,
        pub(crate) unk_2: i16,
        pub(crate) unk_3: i32,
        pub(crate) unk_4: i32,
        pub(crate) unk_5: i16
    }

    #[cfg(test)]
    mod tests {
        use std::mem::size_of;
        use super::*;
        #[test]
        fn test_size_pic_data() {
            assert_eq!(size_of::<Header>(), 0x20);
            assert_eq!(size_of::<Label1Header>(), 0x10);

            assert_eq!(size_of::<PictureData>(), 14);
            assert_eq!(size_of::<i32>() * 2 + size_of::<i16>() * 3, size_of::<PictureData>());



        }
    }

    impl PictureData{
        byteswap!(unk_1, unk_2, unk_3, unk_4, unk_5);
    }
}

#[derive(Error, Debug)]
pub enum TextCommandError {
    #[error("text command was cut off")]
    CutOff,

    #[error("Unable to find end of text command")]
    NoEnd,
    #[error("invalid or unimplemented text command found in unit {0}: {1:#06x} \ntext builder result before invalid value: {2} ")]
    InvalidOrUnimplemented(String, u16, String),
    #[error("invalid or unimplemented text modifier found: {0:#06x}")]
    InvalidTextCommand(u16),
    #[error("invalid text command name : \"{0}\"")]
    InvalidCommandName(String),
    #[error("invalid value whilest trying to parse text: {0}")]
    InvalidValue(String, Option<Box<dyn Error>>)
}

#[derive(Error, Debug)]
pub enum LoadError {
    #[error("unable to find {0} header")]
    UnableToFindHeader(&'static str),
    #[error("unable to find {0} table")]
    UnableToFindTable(&'static str),
    #[error("{0} table is too small")]
    TableTooSmall(&'static str),
    #[error("{0} table element could not be found")]
    TableElementNotFound(&'static str),
    #[error("invalid table element element in table: {0}")]
    InvalidTableElement(&'static str),
    #[error("header contains invalid data: {0}")]
    InvalidHeader(file_repr::HeaderValidationError),
    #[error("label 1 header contains invalid data: {0}")]
    InvalidLabel1Header(file_repr::HeaderValidationError),
    #[error("attribute 1 header contains invalid data: {0}")]
    InvalidAttribute1Header(file_repr::HeaderValidationError),
    #[error("text 2 header contains invalid data: {0}")]
    InvalidText2Header(file_repr::HeaderValidationError),
    #[error("error whiles reading label")]
    LabelRead,
    #[error("unable to decode text command: {0}")]
    TextCommand(TextCommandError),
    #[error("not enough texts found to match labels")]
    NotEnoughTexts,
}
macro_rules! ensure_size {
    (($slice:ident <= $size:expr) => $err:expr ) => {
        if($slice.len() <= $size as usize){
            return Err($err);
        }
    };

    (($slice:ident == $size:expr) => $err:expr ) => {
        if($slice.len() == $size as usize){
            return Err($err);
        }
    };

    (($slice:ident < $size:expr) => $err:expr ) => {
        if($slice.len() < $size as usize){
            return Err($err);
        }
    };

    (($slice:ident != $size:expr) => $err:expr ) => {
        if($slice.len() != $size as usize){
            return Err($err);
        }
    };
}

macro_rules! ensure_allign {
    ($begin_slice:ident ,$slice:ident => $err:expr ) => {
        ensure_size!(($slice < 1) => $err);
        ensure_size!(($begin_slice != 0) => $err);
    };
}

macro_rules! byteswap_manual_inline {
    ($val:ident, $bom:ident) => {
        if  $bom == file_repr::BIG_ENDIAN && cfg!(target_endian = "little") ||
            $bom == file_repr::LITTLE_ENDIAN && cfg!(target_endian = "big"){
            $val.swap_bytes()
        } else {
            $val
        }
    };
}

macro_rules! byteswap_manual {
    ($val:ident, $bom:ident) => {
        $val = byteswap_manual_inline!($val, $bom)
    };
}

macro_rules! special_translation_unit{
    (
        $name:ident {
            $(($hex:literal, $disp:literal) => ($($passoff_unit:tt)::*)),*
        }
    ) =>{
        mod $name{
            use $crate::msbt::TextCommandError;
            use $crate::msbt::file_repr;
            use $crate::msbt::TextCommandError::{InvalidCommandName, InvalidOrUnimplemented, CutOff};

            pub(crate) fn bin_to_text(iter: &mut impl Iterator<Item = u16>, bom: [u8;2]) -> Result<String, TextCommandError>{
                let mut val = iter.next().ok_or(CutOff)? ;
                byteswap_manual!(val, bom);
                let res = match val{
                    $(
                        $hex =>  ($($passoff_unit::)*bin_to_text(iter, bom), $disp),
                    )*

                    unmatched_val => return Err(InvalidOrUnimplemented(stringify!( $name ).into(), unmatched_val, String::new())),
                };

                match res.0{
                    Ok(v) => Ok(format!("{}{}", res.1, v )),
                    Err(e) => match e{
                        InvalidOrUnimplemented(name, val, str) => Err(InvalidOrUnimplemented(name, val, format!("{}{}", res.1 , str))),
                        e => Err(e)
                    }
                }
            }

            pub(crate) fn text_to_bin(data: &str, bom: [u8;2]) -> Result<Vec<u16>, TextCommandError>{
                let val = $(
                if data.starts_with($disp){
                    let newval: u16 = $hex;

                    let mut out: Vec<u16> = vec![byteswap_manual_inline!(newval, bom)];

                    out.append(&mut $($passoff_unit::)*text_to_bin(&data[$disp.len()..], bom)?);

                    out
                } else
                )*
                {
                    return Err(InvalidCommandName(data.to_owned()))
                };

                Ok(val)
            }
        }
    }
}

pub struct Msbt {
    // we will be storing the header, full label1 section and full attribute1 section as
    // we do now have enough information on them to decode and regenerate them
    header: Header,
    label1_header: Label1Header,
    label1_section: Vec<u8>,
    attribute1_header: Attribute1Header,
    attribute1_section: Vec<u8>,
    pub attributes: Vec<Vec<u8>>,

    pub labels: Vec<Label>,
    pub texts: Vec<String>,
}

pub struct Label {
    pub name: String,
    pub index: u32,
}

mod finish_unit{
    use crate::msbt::TextCommandError;

    pub(crate) fn bin_to_text(_iter: &mut impl Iterator<Item = u16>, _bom: [u8;2]) -> Result<String, TextCommandError> {
        Ok(String::new())
    }

    pub(crate) fn text_to_bin(_data: &str, _bom: [u8;2]) -> Result<Vec<u16>, TextCommandError>{
        Ok(Vec::new())
    }
}

mod i16_value_unit{
    use bytemuck::{cast, cast_slice};
    use crate::msbt::TextCommandError;
    use crate::msbt::TextCommandError::{CutOff, NoEnd};
    use crate::msbt::file_repr;

    pub(crate) fn bin_to_text(iter: &mut impl Iterator<Item = u16>, _bom: [u8;2]) -> Result<String, TextCommandError> {
        Ok(cast::<_,i16>(iter.next().ok_or(CutOff)?).to_string())
    }

    pub(crate) fn text_to_bin(data: &str, bom: [u8; 2]) -> Result<Vec<u16>, TextCommandError>{
        //let len = data.find(">").ok_or(NoEnd)?;
        //let data = &data[..len];
        let mut val = data.parse::<i16>().map_err(|e| TextCommandError::InvalidValue(data.to_owned(), Some(Box::new(e))))?;

        byteswap_manual!(val, bom);

        Ok(vec![cast(val)])
    }
}

mod i32_value_unit{
    use bytemuck::{cast, cast_slice};
    use crate::msbt::TextCommandError;
    use crate::msbt::TextCommandError::{CutOff, NoEnd};
    use crate::msbt::file_repr;

    pub(crate) fn bin_to_text(iter: &mut impl Iterator<Item = u16>, _bom: [u8;2]) -> Result<String, TextCommandError> {
        Ok(cast::<_,i32>(iter.next().ok_or(CutOff)?).to_string())
    }

    pub(crate) fn text_to_bin(data: &str, bom: [u8; 2]) -> Result<Vec<u16>, TextCommandError>{
        //let len = data.find(">").ok_or(NoEnd)?;
        //let data = &data[..len];
        let mut val = data.parse::<i32>().map_err(|e| TextCommandError::InvalidValue(data.to_owned(), Some(Box::new(e))))?;

        byteswap_manual!(val, bom);

        Ok(vec![cast(val)])
    }
}


special_translation_unit!(font_large_unit{
   (0x0002, "Size=") => (crate::msbt::i16_value_unit)
});

special_translation_unit!(font_normal_unit{
    //todo: add a propper color unit instead of just using displaying the color id raw to the user
   (0x0002, "Color=") => (crate::msbt::i16_value_unit)
});

special_translation_unit!(text_modifier_unit {
    (0x0002, "FontLarge.") => (crate::msbt::font_large_unit),
    (0x0003, "FontNormal.") => (crate::msbt::font_normal_unit),
    (0x0004, "WaitA") => (crate::msbt::finish_unit)
});

/*
fn decompile_text_modifier(data: &mut impl Iterator<Item=u16>, bom: [u8; 2]) -> Result<String, TextCommandError>{
        Ok(match data.next().map(|v| byteswap_manual_inline!(v, bom)){
            Some(0x0003) => {
                String::from("FontNormal")
            },
            Some(0x0002) => {
                String::from("FontLarge")
            },
            Some(0x0004) => {
                String::from("WaitA")
            },
            Some(i) => {
                return Err(InvalidTextModifier(i));
            },
            None => {
                return Err(CutOff)
            }
        })
    }
 */

mod symbol_unit{
    use std::mem::size_of;
    use bytemuck::{cast, cast_slice};
    use crate::msbt::file_repr::PictureData;
    use crate::msbt::TextCommandError;
    use crate::msbt::TextCommandError::{CutOff, InvalidValue};

    pub(crate) fn bin_to_text(iter: &mut impl Iterator<Item = u16>, bom: [u8;2]) -> Result<String, TextCommandError> {
        let mut data: Vec<u16> = Vec::new();

        for _ in 0..size_of::<PictureData>()/2{
            data.push(iter.next().ok_or(CutOff)?);
        }

        let mut data: PictureData =  cast_slice(&data)[0];

        data.byteswap(bom);

        let PictureData {
            unk_1,
            unk_2,
            unk_3,
            unk_4,
            unk_5
        } = data;

        Ok(format!("Pict{{ {},{},{},{},{} }}", unk_1, unk_2, unk_3, unk_4, unk_5))
    }

    pub(crate) fn text_to_bin(data: &str, bom: [u8;2]) -> Result<Vec<u16>, TextCommandError>{
        if !data.starts_with("Pict{") || !data.ends_with("}"){
            return Err(InvalidValue(data.to_owned(), None));
        }

        let data = &data[5.. data.len()-1];

        let data_split: Vec<_> = data.split(",")
            .map(|d| d.replace(" ", "")).collect();

        if !data_split.len() == 5 {
            return Err(InvalidValue(data.to_owned(), None));
        }

        let unk_1 = data_split[0].parse::<i16>().map_err(|e| InvalidValue(data.to_owned(), Some(Box::new(e))))?;
        let unk_2 = data_split[1].parse::<i16>().map_err(|e| InvalidValue(data.to_owned(), Some(Box::new(e))))?;
        let unk_3 = data_split[2].parse::<i32>().map_err(|e| InvalidValue(data.to_owned(), Some(Box::new(e))))?;
        let unk_4 = data_split[3].parse::<i32>().map_err(|e| InvalidValue(data.to_owned(), Some(Box::new(e))))?;
        let unk_5 = data_split[4].parse::<i16>().map_err(|e| InvalidValue(data.to_owned(), Some(Box::new(e))))?;

        let mut pic_dat = PictureData{
            unk_1,
            unk_2,
            unk_3,
            unk_4,
            unk_5
        };

        pic_dat.byteswap(bom);

        Ok(cast_slice(&[pic_dat]).to_owned())
    }
}

special_translation_unit!(
    base_unit {
        (0x0000, "mod.") => (crate::msbt::text_modifier_unit),
        (0x0001, "sym.") => (crate::msbt::symbol_unit),
        (0x0004, "insert:") => (crate::msbt::i32_value_unit)
    }
);

/*
fn decompile_text_command(data: &mut impl Iterator<Item=u16>, bom: [u8; 2]) -> Result<String, TextCommandError> {
        let command_decompilation = match data.next().map(|v| byteswap_manual_inline!(v, bom)) {
            Some(0x0000) => {
                format!("mod.{}", Self::decompile_text_modifier(data,bom)?)
            },
            Some(0x0001) => {
                String::from("sym.")
            },
            Some(i) => {
                return Err(InvalidOrUnimplemented(i));
            },
            None => {
                return Err(CutOff);
            }
        };

        let str = format!("<{}>", command_decompilation);

        Ok(str)
    }
 */


macro_rules! static_assert {
    ($cond:expr) => {
        #[allow(dead_code)]
        const fn static_assertion() {
            assert!($cond);
        }

        const _: () = static_assertion();
    }
}


//static_assert!(core::mem::size_of::<char>() == 4);

impl Msbt {
    fn allign_offset(offset: &mut usize, allignment: usize) {
        println!("offset % allignment: {}",  *offset % allignment);
        if (*offset % allignment) != 0 {
            *offset += allignment - (*offset % allignment);
        }

        assert_eq!(*offset % allignment, 0);
    }

    fn generate(&self) -> Vec<u8> {
        vec![]
    }

    fn read_label(label_section: &[u8], offset: &mut usize, bom: [u8; 2]) -> Result<Label, LoadError> {
        let label_size = label_section[*offset];
        let label_begin = &label_section[*offset + 1..];
        let content = String::from_utf8(label_begin[0..label_size as usize].to_vec()).map_err(|_e| InvalidTableElement("label table"))?;

        let id_vec = &label_begin[label_size as usize..label_size as usize + 4].to_vec();

        let label_id: &[u32] = cast_slice(id_vec);

        let mut label_id = label_id[0];

        *offset += 1 + label_size as usize + 4;

        byteswap_manual!(label_id, bom);
        Ok(Label {
            name: content,
            index: label_id,
        })
    }

    fn read_label_table(label_section: &[u8], bom: [u8; 2]) -> Result<Vec<Label>, LoadError> {
        let mut labels: Vec<Label> = Vec::new();

        let label_count: &[u32] = cast_slice(&label_section[..size_of::<u32>()]);

        let mut label_count = label_count[0];
        byteswap_manual!(label_count, bom);
        let label_count = label_count as usize;

        let label_nodes: &[LabelOffsetTableNode] = try_cast_slice(&label_section[
            size_of::<u32>()..
            size_of::<u32>()+label_count*size_of::<LabelOffsetTableNode>()
            ]).unwrap();

        ensure_size!((label_nodes < label_count) => UnableToFindTable("label offset"));

        for label_node in &label_nodes[0..label_count] {
            let mut node = *label_node;
            node.check_and_correct(bom).unwrap();

            let mut read_offset: usize = node.offset as usize;
            for _ in 0..node.count {
                labels.push(Self::read_label(label_section, &mut read_offset, bom)?);
            }
        }


        Ok(labels)
    }





    fn read_text(text: &[u8], bom: [u8; 2]) -> Result<String, LoadError> {
        let mut str = String::new();

        let text: &[u16] = cast_slice(text);

        let mut iter = text.iter().copied();

        while let Some(mut c) = iter.next() {
            byteswap_manual!(c, bom);

            if c == 0x0 {
                break;
            }
            //let converted = String::from_utf16(&[char]).map_err(|_e| InvalidTableElement("text table"))?;

            if c == 0x000E {
                str += "<";

                str += &base_unit::bin_to_text(&mut iter, bom).map_err(TextCommand)?; //&Self::decompile_text_command(&mut iter, bom).map_err(TextCommand)?;

                str += ">";

                continue;
            }

            if let Some(c) = char::decode_utf16([c]).next() {
                if let Ok(c) = c {
                    str.push(c);
                } else {
                    return Err(InvalidTableElement("text table"));
                }
            } else {
                return Err(InvalidTableElement("text table"));
            }
            //str.;
        }


        Ok(str)
    }

    fn read_text_table(text_table: &[u8], bom: [u8; 2]) -> Result<Vec<String>, LoadError> {
        let mut out: Vec<String> = Vec::new();

        let text_count: &[u32] = cast_slice(text_table);

        let mut text_count = text_count[0];

        byteswap_manual!(text_count, bom);

        let text_offsets: &[u32] = cast_slice(&text_table[size_of::<u32>()..]);

        ensure_size!((text_offsets < text_count) => UnableToFindTable("text offset"));

        for text_offset in &text_offsets[0..text_count as usize] {
            let mut node = *text_offset;
            byteswap_manual!(node, bom);

            out.push(Self::read_text(&text_table[node as usize..], bom)?);
        }

        Ok(out)
    }

    fn read_attribute_table(data: &[u8], bom: [u8; 2]) -> Result<Vec<Vec<u8>>, LoadError>{
        let u32_data: &[u32] = cast_slice(data);

        let mut count = u32_data[0];
        byteswap_manual!(count, bom);
        let count = count as usize;
        let mut attrib_size = u32_data[1];
        byteswap_manual!(attrib_size, bom);
        let attrib_size = attrib_size as usize;

        let mut  attr_data: Vec<Vec<u8>> = Vec::new();

        for i in 0..count{
            let offset = attrib_size * i + 2 * size_of::<u32>();

            attr_data.push(data[offset.. offset + attrib_size].to_owned());
        }

        Ok(attr_data)
    }
    
    

    //todo: make use try_cast_slice instead of cast_slice in order to avoid panics(its not fine to panic when a user tries to load a file as thats expected to happen)
    //todo: also ensure the slices are cut off propperly before passing them into cast_slice to prevent slop errors (this has already been done on some lines)
    pub fn load(data: &[u8]) -> Result<Msbt, LoadError> {
        let mut offset_counter = 0usize;

        let header_data: &[Header] = cast_slice(&data[..size_of::<Header>()]);

        let mut header = header_data[0];
        header.check_and_correct().map_err(InvalidHeader)?;

        offset_counter += size_of::<Header>();

        let header_data: &[Label1Header] = cast_slice(&data[offset_counter..offset_counter+size_of::<Label1Header>()]);

        let mut label1_header = header_data[0];
        label1_header.check_and_correct(header.bom).map_err(InvalidLabel1Header)?;

        offset_counter += size_of::<Label1Header>();

        //let mut offset_nodes: Vec<LabelOffsetTableNode> = Vec::with_capacity(label1_header.table_size as usize / size_of::<LabelOffsetTableNode>());


        /*ensure_allign!(begin, node_data => UnableToFindTable("label offset"));
        ensure_size!((node_data < (label1_header.table_size as usize / size_of::<LabelOffsetTableNode>())) => UnableToFindTable("label offset"));*/

        let mut labels = Self::read_label_table(&data[offset_counter..], header.bom)?;

        /*for node in &node_data[0..label1_header.table_size as usize / size_of::<LabelOffsetTableNode>()] {
            offset_nodes.push(*node);
        }*/

        let label1_section = data[offset_counter..offset_counter + label1_header.table_size as usize + 1].to_owned();

        offset_counter += label1_header.table_size as usize;

        Self::allign_offset(&mut offset_counter, 0x10);

        let header_data: &[Attribute1Header] = cast_slice(&data[offset_counter..offset_counter + size_of::<Attribute1Header>()]);

        let mut attribute1_header = header_data[0];

        attribute1_header.check_and_correct(header.bom).map_err(InvalidAttribute1Header)?;

        offset_counter += size_of::<Attribute1Header>();

        let attributes = Self::read_attribute_table(&data[offset_counter..], header.bom)?;


        let attribute1_section = data[offset_counter..offset_counter + attribute1_header.table_size as usize].to_owned();

        offset_counter += attribute1_header.table_size as usize;

        Self::allign_offset(&mut offset_counter, 0x10);

        let header_data: &[Text2Header] =
            cast_slice(&data[offset_counter..]);

        //ensure_allign!(begin, header_data => UnableToFindHeader("Text 1"));

        let mut text2_header = header_data[0];
        text2_header.check_and_correct(header.bom).map_err(InvalidText2Header)?;
        offset_counter += size_of::<Text2Header>();

        let texts = Self::read_text_table(&data[offset_counter..], header.bom)?;

        let max_val = labels.iter().map(|l| l.index).max().unwrap_or(0);

        if texts.len() < max_val as usize {
            return Err(NotEnoughTexts);
        }

        labels.sort_by(|l, r| match l.name.len().cmp(&r.name.len()) {
            Ordering::Equal => {
                l.name.cmp(&r.name)
            }
            o => o
        });

        Ok(Msbt {
            header,
            label1_header,
            label1_section,
            attribute1_section,
            attribute1_header,
            labels,
            texts,
            attributes
        })
    }

    pub fn resize_to_allign_end<T: num::Num>(vec: &mut Vec<T>){
        while vec.len() * size_of::<T>() % 0x10 != 0{
            vec.push(T::zero());

        }
    }

    pub fn build(&self) -> Result<Vec<u8>,TextCommandError>{
        let bom = self.header.bom;

        let mut data: Vec<u8> = Vec::new();

        let mut header = self.header;

        header.byteswap(header.bom);

        data.extend_from_slice(cast_slice(&[header]));

        let mut label1_header = self.label1_header;

        label1_header.byteswap(header.bom);

        data.extend_from_slice(cast_slice(&[label1_header]));
        data.extend_from_slice(&self.label1_section);

        Self::resize_to_allign_end(&mut data);



        let mut attrib_header = self.attribute1_header;



        let mut attr1_sect: Vec<u8> = Vec::new();
        let mut attrib_count = self.attributes.len() as u32;
        byteswap_manual!(attrib_count, bom);
        attr1_sect.extend_from_slice(cast_slice(&[attrib_count]));
        let mut attrib_length = self.attributes.first().map(|a| a.len()).unwrap_or(0) as u32;
        byteswap_manual!(attrib_length, bom);
        attr1_sect.extend_from_slice(cast_slice(&[attrib_length]));


        for attrib in &self.attributes{
            attr1_sect.extend_from_slice(attrib);
        }
        Self::resize_to_allign_end(&mut attr1_sect);

        attrib_header.table_size = attr1_sect.len() as u32;

        attrib_header.byteswap(header.bom);

        data.extend_from_slice(cast_slice(&[attrib_header]));

        data.extend(attr1_sect);

        //fill up rest to match table length
        /*let mut len = size_of::<u32>() as u32 * 2 + attrib_length * self.attributes.len() as u32;

        while len != self.attribute1_header.table_size{
            attr1_sect.push(0);
            len += 1;
        }*/

        let mut text_offsets: Vec<u32> = vec![0; self.texts.len() + 1];

        let mut text_length = self.texts.len() as u32;

        byteswap_manual!(text_length, bom);

        text_offsets[0] = text_length;

        let mut texts: Vec<u16> = Vec::new();



        Self::resize_to_allign_end(&mut text_offsets);



        for i in 0..self.texts.len(){
            let mut str: &str = &self.texts[i];

            let mut offset_val = (text_offsets.len() * size_of::<u32>() + texts.len() * size_of::<u16>()) as u32;
            byteswap_manual!(offset_val, bom);

            text_offsets[1 + i] = offset_val;

            while let Some(pos) = str.find("<"){
                texts.extend(str[..pos].encode_utf16().map(|v| byteswap_manual_inline!(v, bom)));

                str = &str[pos+1..];



                let Some(end_pos) = str.find(">") else {
                    return Err(NoEnd);
                };

                texts.extend_from_slice(cast_slice::<u8,_>(&[0x00,0x0e]));

                texts.extend(base_unit::text_to_bin(&str[..end_pos], self.header.bom)?);

                str = &str[end_pos+1..];
            }

            texts.extend(str.encode_utf16().map(|v| byteswap_manual_inline!(v, bom)));

            texts.push(0);

            Self::resize_to_allign_end(&mut texts);
        }

        let mut text_table_header = Text2Header{
            magic: Text2Header::MAGIC,
            table_size: (text_offsets.len() * size_of::<u32>() + texts.len() * size_of::<u16>()) as u32,
            _padding: 0
        };

        text_table_header.byteswap(self.header.bom);

        data.extend_from_slice(cast_slice(&[text_table_header]));
        data.extend_from_slice(cast_slice(&text_offsets));
        data.extend_from_slice(cast_slice(&texts));

        let len = [data.len()];
        let size_data: &[u8] = cast_slice(&len);
        for i in 0usize..4usize{
            data[offset_of!(Header, file_size)+i] = size_data[i];
        }

        Ok(data)
    }
}
