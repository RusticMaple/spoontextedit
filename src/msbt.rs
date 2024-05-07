use std::cmp::Ordering;
use std::mem::size_of;
use std::string::FromUtf8Error;
use thiserror::Error;
use crate::msbt::file_repr::{Attribute1Header, Header, Label1Header, LabelOffsetTableNode, Text2Header};
use crate::msbt::LoadError::{InvalidAttribute1Header, InvalidHeader, InvalidLabel1Header, InvalidText2Header, LabelRead, InvalidTableElement, TableElementNotFound, UnableToFindHeader, UnableToFindTable, NotEnoughTexts};

mod file_repr {
    use thiserror::Error;
    use crate::msbt::file_repr::HeaderValidationError::{InvalidBom, InvalidMagic};
    use crate::msbt::LoadError;

    pub const BIG_ENDIAN: [u8; 2] = [0xFE, 0xFF];
    pub const LITTLE_ENDIAN: [u8; 2] = [0xFF, 0xFE];
    macro_rules! valid_values {
        (($var:expr) == !{$($val:path),*} => $err:expr ) => {
            if !($( $var == $val || )* false) {
                return Err($err);
            }
        };
    }

    macro_rules! byteswap {
        ($( $val:ident ),*) => {
            fn byteswap(&mut self, bom: [u8;2]){
                if  bom == BIG_ENDIAN && cfg!(target_endian = "little") ||
                    bom == LITTLE_ENDIAN && cfg!(target_endian = "big"){
                    $(
                    self.$val = self.$val.swap_bytes();
                    )*
                }
            }
        };
    }

    #[derive(Clone, Copy)]
    pub struct Version(u8, u8);

    #[repr(C, packed)]
    #[derive(Copy, Clone)]
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
        #[error("Invalid Magic")]
        InvalidMagic,
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
            valid_values!((self.magic) == !{ Self::MAGIC } => InvalidMagic);
            valid_values!((self.bom) == !{ BIG_ENDIAN, LITTLE_ENDIAN } => InvalidBom);


            self.byteswap(self.bom);

            Ok(())
        }

        byteswap!(file_size,sections);
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
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
            valid_values!((self.magic) == !{Self::MAGIC} => InvalidMagic);

            self.byteswap(bom);

            Ok(())
        }

        byteswap!(table_size);
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
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
    #[derive(Copy, Clone)]
    pub(crate) struct Attribute1Header{
        magic: [u8; 4],
        pub table_size: u32,
        _padding: u64
    }

    impl Attribute1Header{
        const MAGIC: [u8; 4] = [
            'A' as u8,
            'T' as u8,
            'R' as u8,
            '1' as u8, ];

        pub fn check_and_correct(&mut self, bom: [u8;2]) -> Result<(),HeaderValidationError> {
            self.byteswap(bom);

            valid_values!((self.magic) == !{Self::MAGIC} => InvalidMagic);

            Ok(())
        }

        byteswap!(table_size);
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub(crate) struct Text2Header{
        magic: [u8;4],
        table_size: u32,
        _padding: u64
    }

    impl Text2Header{
        const MAGIC: [u8; 4] = [
            'T' as u8,
            'X' as u8,
            'T' as u8,
            '2' as u8, ];

        pub fn check_and_correct(&mut self, bom: [u8;2]) -> Result<(),HeaderValidationError>{
            self.byteswap(bom);

            valid_values!((self.magic) == !{Self::MAGIC} => InvalidMagic);

            Ok(())
        }

        byteswap!(table_size);
    }
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

macro_rules! byteswap_manual {
    ($val:ident, $bom:ident) => {
        if  $bom == file_repr::BIG_ENDIAN && cfg!(target_endian = "little") ||
            $bom == file_repr::LITTLE_ENDIAN && cfg!(target_endian = "big"){
            $val = $val.swap_bytes();
        }
    };
}

pub struct Msbt {
    // we will be storing the header, full label1 section and full attribute1 section as
    // we do now have enough information on them to decode and regenerate them
    header: Header,
    label1_header: Label1Header,
    label1_section: Vec<u8>,
    attribute1_header: Attribute1Header,
    attribute1_section: Vec<u8>,

    pub labels: Vec<Label>,
    pub texts: Vec<String>
}

pub struct Label{
    pub name: String,
    pub index: u32
}

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
    fn allign_offset(offset: &mut usize, allignment: usize){
        if (*offset % allignment) != 0{
            *offset += allignment - (*offset % allignment);
        }

        assert_eq!(*offset % allignment, 0);
    }

    fn generate(&self) -> Vec<u8> {
        vec![]
    }

    fn read_label(label_section: &[u8], offset: &mut usize, bom: [u8;2]) -> Result<Label,LoadError>{
        let label_size = label_section[*offset];
        let label_begin = &label_section[*offset+1..];
        let content = String::from_utf8(label_begin[0..label_size as usize].to_vec()).map_err(|_e| InvalidTableElement("label table"))?;

        let id_vec = &label_begin[label_size as usize..label_size as usize + 4].to_vec();

        let (begin, label_id, rest) = unsafe {
            id_vec.align_to::<u32>()
        };

        ensure_allign!(begin, label_id => LabelRead);

        let mut label_id = label_id[0];

        *offset += 1 + label_size as usize + 4;

        byteswap_manual!(label_id, bom);

        Ok(Label{
            name: content,
            index: label_id
        })
    }

    fn read_label_table(label_section: &[u8], bom: [u8; 2]) -> Result<Vec<Label>, LoadError>{
        let mut labels: Vec<Label> = Vec::new();

        let (begin, label_count, rest) = unsafe {
            label_section
                .align_to::<u32>()
        };

        ensure_allign!(begin, label_count => UnableToFindTable("label offset"));

        let mut label_count = label_count[0];

        byteswap_manual!(label_count, bom);

        let (begin, label_nodes, rest) = unsafe {
            label_section[size_of::<u32>()..]
                .align_to::<LabelOffsetTableNode>()
        };


        ensure_allign!(begin, label_nodes => UnableToFindTable("label offset"));
        ensure_size!((label_nodes < label_count) => UnableToFindTable("label offset"));

        for label_node in &label_nodes[0..label_count as usize]{
            let mut node = *label_node;
            node.check_and_correct(bom).unwrap();

            let mut read_offset: usize = node.offset as usize;
            for _ in 0..node.count {
                labels.push(Self::read_label(label_section, &mut read_offset, bom)?);
            }
        }


        Ok(labels)
    }

    fn read_text(text: &[u8], bom: [u8; 2]) -> Result<String,LoadError> {
        let mut str = String::new();

        let (begin, text, rest) = unsafe {
            text
                .align_to::<u16>()
        };

        ensure_allign!(begin, text => TableElementNotFound("text table"));

        let mut iter = text.iter().copied();

        while let Some(mut c) = iter.next(){
            byteswap_manual!(c, bom);

            if c == 0x0{
                break;
            }
            //let converted = String::from_utf16(&[char]).map_err(|_e| InvalidTableElement("text table"))?;

            if let Some(c) = char::decode_utf16([c]).next(){
                if let Ok(c) = c{
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

    fn read_text_table(text_table: &[u8], bom: [u8; 2]) -> Result<Vec<String>,LoadError> {
        let mut out: Vec<String> = Vec::new();

        let (begin, text_count, rest) = unsafe {
            text_table
                .align_to::<u32>()
        };

        ensure_allign!(begin, text_count => UnableToFindTable("table offset"));

        let mut text_count = text_count[0];

        byteswap_manual!(text_count, bom);

        let (begin, text_offsets, rest) = unsafe {
            text_table[size_of::<u32>()..]
                .align_to::<u32>()
        };

        ensure_allign!(begin, text_offsets => UnableToFindTable("text offset"));
        ensure_size!((text_offsets < text_count) => UnableToFindTable("text offset"));

        for text_offset in &text_offsets[0..text_count as usize]{
            let mut node = *text_offset;
            byteswap_manual!(node, bom);

            out.push(Self::read_text(&text_table[node as usize..], bom)?);
        }

        Ok(out)
    }

    pub fn load(data: &[u8]) -> Result<Msbt, LoadError> {

        let mut offset_counter = 0usize;

        let (begin, header_data, rest) = unsafe { data.align_to::<Header>() };

        ensure_allign!(begin, header_data => UnableToFindHeader("Msbt"));
        let mut header = header_data[0];
        header.check_and_correct().map_err(InvalidHeader)?;

        offset_counter += size_of::<Header>();

        let (begin, header_data, rest) = unsafe {
            data[offset_counter..]
                .align_to::<Label1Header>()
        };

        ensure_allign!(begin, header_data => UnableToFindHeader("Label 1"));
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

        let label1_section = data[offset_counter..offset_counter + label1_header.table_size as usize].to_owned();

        offset_counter += label1_header.table_size as usize;

        Self::allign_offset(&mut offset_counter, 0x10);

        let (begin, header_data, rest) = unsafe {
            data[offset_counter..]
                .align_to::<Attribute1Header>()
        };

        ensure_allign!(begin, header_data => UnableToFindHeader("Attribute 1"));

        let mut attribute1_header = header_data[0];

        attribute1_header.check_and_correct(header.bom).map_err(InvalidAttribute1Header)?;

        offset_counter += size_of::<Attribute1Header>();

        let attribute1_section = data[offset_counter..offset_counter +  attribute1_header.table_size as usize].to_owned();

        offset_counter += attribute1_header.table_size as usize;

        Self::allign_offset(&mut offset_counter, 0x10);

        let (begin, header_data, rest) = unsafe {
            data[offset_counter..]
                .align_to::<Text2Header>()
        };

        ensure_allign!(begin, header_data => UnableToFindHeader("Text 1"));

        let mut text2_header = header_data[0];
        text2_header.check_and_correct(header.bom).map_err(InvalidText2Header)?;
        offset_counter += size_of::<Text2Header>();

        let texts = Self::read_text_table(&data[offset_counter..], header.bom)?;

        let max_val = labels.iter().map(|l| l.index).max().unwrap_or(0);

        if texts.len() < max_val as usize{
            return Err(NotEnoughTexts);
        }
        
        labels.sort_by(|l,r| match l.name.len().cmp(&r.name.len()){
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
            texts
        })
    }
}