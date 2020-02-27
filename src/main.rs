#[macro_use]
extern crate log;

use std::env;
use std::fs::File;
use std::io::{self, prelude::*};
use wasmparser::{
    ExternalKind, FuncType, ImportSectionEntryType, MemoryType, Operator, Parser, ParserState,
    ResizableLimits, SectionCode, Type, WasmDecoder,
};

fn main() {
    drop(env_logger::init());
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        println!("Usage: {} <input wasm file> <output module name>", args[0]);
        return;
    }
    let buf: Vec<u8> = read_wasm(&args[1]).unwrap();
    let mut glue_file = File::create(format!("{}_glue.h", args[2])).expect("create glue file");
    glue_file
        .write_all(
            b"#include<stddef.h>
#include<stdint.h>\n\n",
        )
        .expect("write glue file");
    glue_file
        .write_all(
            b"const uint64_t functionDefMutableData = 0;
const uint64_t biasedInstanceId = 0;\n\n",
        )
        .expect("write glue file");

    let mut parser = Parser::new(&buf);
    let mut section_name: Option<String> = None;
    let mut type_entries: Vec<FuncType> = vec![];
    let mut next_import_index = 0;
    let mut next_function_index = 0;
    let mut function_entries: Vec<Option<usize>> = vec![];
    let mut has_main = false;
    let mut memories: Vec<Vec<u8>> = vec![];
    let mut data_index: Option<usize> = None;
    let mut data_offset: Option<usize> = None;
    loop {
        let state = parser.read();
        match *state {
            ParserState::BeginSection { code, .. } => {
                if let SectionCode::Custom { name, .. } = code {
                    section_name = Some(name.to_string());
                }
            }
            ParserState::EndSection => {
                section_name = None;
            }
            ParserState::SectionRawData(data) => {
                if section_name.clone().unwrap_or("".to_string()) == "wavm.precompiled_object" {
                    let mut f = File::create(format!("{}.o", args[2])).expect("create object file");
                    f.write_all(data).expect("write object file");
                }
            }
            ParserState::TypeSectionEntry(ref t) => {
                glue_file
                    .write_all(
                        format!("const uint64_t typeId{} = 0;\n", type_entries.len()).as_bytes(),
                    )
                    .expect("write glue file");
                type_entries.push(t.clone());
            }
            ParserState::ImportSectionEntry {
                module,
                field,
                ty: ImportSectionEntryType::Function(index),
            } => {
                function_entries.push(None);
                let func_type = &type_entries[index as usize];
                let name = format!("wavm_{}_{}", module, field);
                let import_symbol = format!("functionImport{}", next_import_index);
                glue_file
                    .write_all(format!("#define {} {}\n", name, import_symbol).as_bytes())
                    .expect("write glue file");
                next_import_index += 1;
                glue_file
                    .write_all(
                        format!(
                            "extern {};\n",
                            convert_func_type_to_c_function(&func_type, import_symbol)
                        )
                        .as_bytes(),
                    )
                    .expect("write glue file");
            }
            ParserState::FunctionSectionEntry(type_entry_index) => {
                let func_type = &type_entries[type_entry_index as usize];
                let name = format!("functionDef{}", next_function_index);
                glue_file
                    .write_all(
                        format!(
                            "extern {};
const uint64_t functionDefMutableDatas{} = 0;\n",
                            convert_func_type_to_c_function(&func_type, name),
                            next_function_index,
                        )
                        .as_bytes(),
                    )
                    .expect("write glue file");
                function_entries.push(Some(next_function_index));
                next_function_index += 1;
            }
            ParserState::ExportSectionEntry {
                field,
                kind: ExternalKind::Function,
                index,
            } => {
                let function_index =
                    function_entries[index as usize].expect("Exported function should exist!");
                glue_file
                    .write_all(
                        format!(
                            "#define wavm_exported_function_{} functionDef{}\n",
                            field, function_index,
                        )
                        .as_bytes(),
                    )
                    .expect("write glue file");

                if field == "_start" {
                    has_main = true;
                }
            }
            ParserState::MemorySectionEntry(MemoryType {
                limits: ResizableLimits { initial: pages, .. },
                ..
            }) => {
                let mut mem = vec![];
                mem.resize(pages as usize * 64 * 1024, 0);
                memories.push(mem);
            }
            ParserState::BeginActiveDataSectionEntry(i) => {
                data_index = Some(i as usize);
            }
            ParserState::EndDataSectionEntry => {
                data_index = None;
                data_offset = None;
            }
            ParserState::InitExpressionOperator(Operator::I32Const { value }) => {
                data_offset = Some(value as usize)
            }
            ParserState::DataSectionEntryBodyChunk(data) => {
                if let (Some(index), Some(offset)) = (data_index, data_offset) {
                    memories[index][offset..offset + data.len()].copy_from_slice(&data);
                }
            }
            ParserState::EndWasm => break,
            ParserState::Error(ref err) => panic!("Error: {:?}", err),
            _ => debug!("Unprocessed states: {:?}", state),
        }
    }

    for (i, mem) in memories.iter().enumerate() {
        glue_file
            .write_all(format!("uint32_t memory{}_length = {};\n", i, mem.len()).as_bytes())
            .expect("write glue file");
        glue_file
            .write_all(format!("uint8_t memory{}[{}] = {{", i, mem.len()).as_bytes())
            .expect("write glue file");
        let reversed_striped_mem: Vec<u8> = mem
            .iter()
            .rev()
            .map(|x| *x)
            .skip_while(|c| *c == 0)
            .collect();
        let striped_mem: Vec<u8> = reversed_striped_mem.into_iter().rev().collect();
        for (j, c) in striped_mem.iter().enumerate() {
            if j % 32 == 0 {
                glue_file.write_all(b"\n  ").expect("write glue file");
            }
            glue_file
                .write_all(format!("0x{:x}", c).as_bytes())
                .expect("write glue file");
            if j < striped_mem.len() - 1 {
                glue_file.write_all(b", ").expect("write glue file");
            }
        }
        glue_file.write_all(b"};\n").expect("write glue file");
        glue_file
            .write_all(format!("#define MEMORY{}_DEFINED 1\n", i).as_bytes())
            .expect("write glue file");
    }

    if has_main {
        glue_file
            .write_all(
                b"\nint main() {
  wavm_exported_function__start(NULL);
  // This should not be reached
  return -1;
}\n",
            )
            .expect("write glue file");
    }
}

fn wasm_type_to_c_type(t: Option<Type>) -> String {
    if let None = t {
        return "void".to_string();
    }
    match t.unwrap() {
        Type::I32 => "int32_t".to_string(),
        Type::I64 => "int64_t".to_string(),
        Type::F32 => "float".to_string(),
        Type::F64 => "double".to_string(),
        _ => panic!("Unsupported type: {:?}", t),
    }
}

fn convert_func_type_to_c_function(func_type: &FuncType, name: String) -> String {
    if func_type.form != Type::Func || func_type.returns.len() > 1 {
        panic!("Invalid func type: {:?}", func_type);
    }
    let mut fields: Vec<String> = func_type
        .params
        .iter()
        .map(|t| wasm_type_to_c_type(Some(*t)))
        .collect();
    fields.insert(0, "void*".to_string());
    format!(
        "{} ({}) ({})",
        wasm_type_to_c_type(func_type.returns.get(0).cloned()),
        name,
        fields.join(", ")
    )
    .to_string()
}

fn read_wasm(file: &str) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    let mut f = File::open(file)?;
    f.read_to_end(&mut data)?;
    Ok(data)
}
