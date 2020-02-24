use std::env;
use std::fs::File;
use std::io::{self, prelude::*};
use wasmparser::{
    ExternalKind, FuncType, ImportSectionEntryType, Parser, ParserState, SectionCode, Type,
    WasmDecoder,
};

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        println!("Usage: {} <input wasm file> <output module name>", args[0]);
        return;
    }
    let buf: Vec<u8> = read_wasm(&args[1]).unwrap();
    let mut glue_file = File::create(format!("{}_glue.c", args[2])).expect("create glue file");
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
                glue_file
                    .write_all(
                        format!(
                            "extern {};\n",
                            convert_func_type_to_c_function(&func_type, name.clone())
                        )
                        .as_bytes(),
                    )
                    .expect("write glue file");
                let import_symbol = format!("*functionImport{}", next_import_index);
                next_import_index += 1;
                glue_file
                    .write_all(
                        format!(
                            "const {} = {};\n",
                            convert_func_type_to_c_function(&func_type, import_symbol),
                            name
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
                            "extern {};\n",
                            convert_func_type_to_c_function(&func_type, name)
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
                            "#define wavm_exported_function_{} functionDef{}
const uint64_t functionDefMutableDatas{} = 0;\n",
                            field, function_index, function_index,
                        )
                        .as_bytes(),
                    )
                    .expect("write glue file");
            }
            ParserState::DataSectionEntryBodyChunk(data) => {
                println!(
                    "{} bytes data section body chunk for {:?}",
                    data.len(),
                    section_name
                );
            }
            ParserState::EndWasm => break,
            ParserState::Error(ref err) => panic!("Error: {:?}", err),
            ParserState::CodeOperator(_) => (),
            _ => println!("{:?}", state),
        }
    }

    glue_file
        .write_all(
            b"\nint main() {
  wavm_exported_function__start();
  // This should not be reached
  return -1;
}\n",
        )
        .expect("write glue file");
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
    let fields: Vec<String> = func_type
        .params
        .iter()
        .map(|t| wasm_type_to_c_type(Some(*t)))
        .collect();
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
