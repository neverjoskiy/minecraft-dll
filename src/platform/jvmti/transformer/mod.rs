use std::{
    fmt,
    sync::{OnceLock, RwLock},
};

use ristretto_classfile::{
    ClassFile,
    attributes::{Attribute, Instruction, StackFrame},
};

const TRAMPOLINE_CLASS: &str = "xz/runtime/NativeHooks4";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalLoad {
    Reference(u8),
    Int(u8),
    Float(u8),
    Long(u8),
    Double(u8),
}

impl LocalLoad {
    fn instruction(self) -> Instruction {
        match self {
            Self::Reference(0) => Instruction::Aload_0,
            Self::Reference(1) => Instruction::Aload_1,
            Self::Reference(2) => Instruction::Aload_2,
            Self::Reference(3) => Instruction::Aload_3,
            Self::Reference(slot) => Instruction::Aload(slot),
            Self::Int(0) => Instruction::Iload_0,
            Self::Int(1) => Instruction::Iload_1,
            Self::Int(2) => Instruction::Iload_2,
            Self::Int(3) => Instruction::Iload_3,
            Self::Int(slot) => Instruction::Iload(slot),
            Self::Float(0) => Instruction::Fload_0,
            Self::Float(1) => Instruction::Fload_1,
            Self::Float(2) => Instruction::Fload_2,
            Self::Float(3) => Instruction::Fload_3,
            Self::Float(slot) => Instruction::Fload(slot),
            Self::Long(0) => Instruction::Lload_0,
            Self::Long(1) => Instruction::Lload_1,
            Self::Long(2) => Instruction::Lload_2,
            Self::Long(3) => Instruction::Lload_3,
            Self::Long(slot) => Instruction::Lload(slot),
            Self::Double(0) => Instruction::Dload_0,
            Self::Double(1) => Instruction::Dload_1,
            Self::Double(2) => Instruction::Dload_2,
            Self::Double(3) => Instruction::Dload_3,
            Self::Double(slot) => Instruction::Dload(slot),
        }
    }

    const fn stack_width(self) -> u16 {
        match self {
            Self::Long(_) | Self::Double(_) => 2,
            _ => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookKind {
    
    Entry,
    
    Gate,
    
    
    Replace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookSpec {
    pub class_names: Vec<String>,
    pub method_name: String,
    pub method_descriptor: String,
    pub callback_name: String,
    pub callback_descriptor: String,
    pub loads: Vec<LocalLoad>,
    pub kind: HookKind,
}

impl HookSpec {
    pub fn entry(
        class_names: impl IntoIterator<Item = impl Into<String>>,
        method_name: impl Into<String>,
        method_descriptor: impl Into<String>,
        callback_name: impl Into<String>,
        callback_descriptor: impl Into<String>,
        loads: Vec<LocalLoad>,
    ) -> Self {
        Self {
            class_names: class_names.into_iter().map(Into::into).collect(),
            method_name: method_name.into(),
            method_descriptor: method_descriptor.into(),
            callback_name: callback_name.into(),
            callback_descriptor: callback_descriptor.into(),
            loads,
            kind: HookKind::Entry,
        }
    }

    pub fn gate(
        class_names: impl IntoIterator<Item = impl Into<String>>,
        method_name: impl Into<String>,
        method_descriptor: impl Into<String>,
        callback_name: impl Into<String>,
        callback_descriptor: impl Into<String>,
        loads: Vec<LocalLoad>,
    ) -> Self {
        let mut spec = Self::entry(
            class_names,
            method_name,
            method_descriptor,
            callback_name,
            callback_descriptor,
            loads,
        );
        spec.kind = HookKind::Gate;
        spec
    }

    pub fn replace(
        class_names: impl IntoIterator<Item = impl Into<String>>,
        method_name: impl Into<String>,
        method_descriptor: impl Into<String>,
        callback_name: impl Into<String>,
        callback_descriptor: impl Into<String>,
        loads: Vec<LocalLoad>,
    ) -> Self {
        let mut spec = Self::entry(
            class_names,
            method_name,
            method_descriptor,
            callback_name,
            callback_descriptor,
            loads,
        );
        spec.kind = HookKind::Replace;
        spec
    }

    fn matches_class(&self, name: &str) -> bool {
        self.class_names.iter().any(|candidate| candidate == name)
    }
}

#[derive(Debug)]
pub enum TransformError {
    ClassFile(String),
    UnsupportedAttribute(&'static str),
    MissingMethod(String),
    OffsetOverflow,
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClassFile(error) => write!(f, "classfile error: {error}"),
            Self::UnsupportedAttribute(name) => {
                write!(f, "cannot safely shift code attribute {name}")
            }
            Self::MissingMethod(method) => write!(f, "target method not found: {method}"),
            Self::OffsetOverflow => f.write_str("instruction offset overflow"),
        }
    }
}

fn hooks() -> &'static RwLock<Vec<HookSpec>> {
    static HOOKS: OnceLock<RwLock<Vec<HookSpec>>> = OnceLock::new();
    HOOKS.get_or_init(|| RwLock::new(Vec::new()))
}

pub fn register_hook(spec: HookSpec) {
    if let Ok(mut hooks) = hooks().write()
        && !hooks.contains(&spec)
    {
        hooks.push(spec);
    }
}

pub fn clear_hooks() {
    if let Ok(mut hooks) = hooks().write() {
        hooks.clear();
    }
}

pub fn has_hook_for(class_name: &str) -> bool {
    hooks()
        .read()
        .map(|hooks| hooks.iter().any(|hook| hook.matches_class(class_name)))
        .unwrap_or(false)
}

pub fn hooked_class_names() -> Vec<String> {
    hooks()
        .read()
        .map(|hooks| {
            let mut names = Vec::new();
            for hook in hooks.iter() {
                for name in &hook.class_names {
                    if !names.iter().any(|existing| existing == name) {
                        names.push(name.clone());
                    }
                }
            }
            names
        })
        .unwrap_or_default()
}

pub fn transform(class_name: &str, bytes: &[u8]) -> Result<Option<Vec<u8>>, TransformError> {
    let specs = hooks()
        .read()
        .map(|hooks| {
            hooks
                .iter()
                .filter(|hook| hook.matches_class(class_name))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if specs.is_empty() {
        return Ok(None);
    }

    let mut class_file = ClassFile::from_bytes(bytes)
        .map_err(|error| TransformError::ClassFile(error.to_string()))?;
    let mut changed = false;

    for spec in &specs {
        match prepend_entry_hook(&mut class_file, spec) {
            Ok(did) => changed |= did,
            
            Err(TransformError::MissingMethod(_)) => continue,
            Err(error) => return Err(error),
        }
    }
    if !changed {
        return Ok(None);
    }

    class_file
        .verify()
        .map_err(|error| TransformError::ClassFile(format!("verify failed: {error}")))?;

    let mut output = Vec::with_capacity(bytes.len() + 128);
    class_file
        .to_bytes(&mut output)
        .map_err(|error| TransformError::ClassFile(error.to_string()))?;
    ClassFile::from_bytes(&output)
        .map_err(|error| TransformError::ClassFile(format!("round-trip failed: {error}")))?
        .verify()
        .map_err(|error| TransformError::ClassFile(format!("round-trip verify failed: {error}")))?;
    Ok(Some(output))
}

fn prepend_entry_hook(
    class_file: &mut ClassFile<'static>,
    spec: &HookSpec,
) -> Result<bool, TransformError> {
    let target_index = class_file
        .methods
        .iter()
        .position(|method| {
            let Ok(name) = class_file.constant_pool.try_get_utf8(method.name_index) else {
                return false;
            };
            let Ok(descriptor) = class_file
                .constant_pool
                .try_get_utf8(method.descriptor_index)
            else {
                return false;
            };
            name.to_str_lossy() == spec.method_name
                && descriptor.to_str_lossy() == spec.method_descriptor
        })
        .ok_or_else(|| {
            TransformError::MissingMethod(format!("{}{}", spec.method_name, spec.method_descriptor))
        })?;

    let trampoline_class = class_file
        .constant_pool
        .add_class(TRAMPOLINE_CLASS)
        .map_err(|error| TransformError::ClassFile(error.to_string()))?;
    let callback = class_file
        .constant_pool
        .add_method_ref(
            trampoline_class,
            &spec.callback_name,
            &spec.callback_descriptor,
        )
        .map_err(|error| TransformError::ClassFile(error.to_string()))?;
    let stack_map_utf8 = class_file
        .constant_pool
        .add_utf8("StackMapTable")
        .map_err(|error| TransformError::ClassFile(error.to_string()))?;

    let method = &mut class_file.methods[target_index];
    let Some(Attribute::Code {
        max_stack,
        code,
        exception_table,
        attributes,
        ..
    }) = method
        .attributes
        .iter_mut()
        .find(|attribute| matches!(attribute, Attribute::Code { .. }))
    else {
        return Err(TransformError::MissingMethod(format!(
            "{}{} has no Code attribute",
            spec.method_name, spec.method_descriptor
        )));
    };

    let trailing = match spec.kind {
        
        HookKind::Gate => 1,
        HookKind::Entry if callback_returns_value(&spec.callback_descriptor) => 1,
        HookKind::Entry | HookKind::Replace => 0,
    };
    if code.iter().take(spec.loads.len() + 1 + trailing).any(
        |instruction| matches!(instruction, Instruction::Invokestatic(index) if *index == callback),
    ) {
        return Ok(false);
    }

    if spec.kind == HookKind::Replace {
        return replace_method_body(max_stack, code, exception_table, attributes, spec, callback);
    }

    ensure_shiftable(attributes)?;

    let mut prefix = spec
        .loads
        .iter()
        .copied()
        .map(LocalLoad::instruction)
        .collect::<Vec<_>>();
    prefix.push(Instruction::Invokestatic(callback));

    
    
    
    let epilogue_index = if spec.kind == HookKind::Gate {
        let index = u16::try_from(prefix.len() + 1 + code.len())
            .map_err(|_| TransformError::OffsetOverflow)?;
        prefix.push(Instruction::Ifne(index));
        Some(index)
    } else if callback_returns_value(&spec.callback_descriptor) {
        prefix.push(Instruction::Pop);
        None
    } else {
        None
    };

    
    
    
    let shift = u16::try_from(prefix.len()).map_err(|_| TransformError::OffsetOverflow)?;
    
    let byte_shift = encoded_len(&prefix)?;

    shift_code(code, shift)?;
    for entry in exception_table {
        entry.range_pc.start = add_offset(entry.range_pc.start, shift)?;
        entry.range_pc.end = add_offset(entry.range_pc.end, shift)?;
        entry.handler_pc = add_offset(entry.handler_pc, shift)?;
    }
    shift_code_attributes(attributes, shift, byte_shift)?;

    let required_stack = spec
        .loads
        .iter()
        .map(|load| load.stack_width())
        .sum::<u16>()
        .max(1);
    *max_stack = (*max_stack).max(required_stack);

    prefix.append(code);
    if let Some(epilogue_index) = epilogue_index {
        prefix.push(Instruction::Return);
        insert_stack_map_at(stack_map_utf8, attributes, epilogue_index)?;
    }
    *code = prefix;
    Ok(true)
}


fn replace_method_body(
    max_stack: &mut u16,
    code: &mut Vec<Instruction>,
    exception_table: &mut Vec<ristretto_classfile::attributes::ExceptionTableEntry>,
    attributes: &mut Vec<Attribute>,
    spec: &HookSpec,
    callback: u16,
) -> Result<bool, TransformError> {
    let mut body = spec
        .loads
        .iter()
        .copied()
        .map(LocalLoad::instruction)
        .collect::<Vec<_>>();
    body.push(Instruction::Invokestatic(callback));

    let target_returns_void = spec.method_descriptor.ends_with('V');
    let callback_returns = callback_returns_value(&spec.callback_descriptor);
    if target_returns_void {
        if callback_returns {
            body.push(Instruction::Pop);
        }
        body.push(Instruction::Return);
    } else if spec.method_descriptor.ends_with('Z') && callback_returns {
        
        body.push(Instruction::Ireturn);
    } else {
        return Err(TransformError::ClassFile(format!(
            "Replace unsupported for {}{}",
            spec.method_name, spec.method_descriptor
        )));
    }

    let required_stack = spec
        .loads
        .iter()
        .map(|load| load.stack_width())
        .sum::<u16>()
        .max(1);
    *max_stack = (*max_stack).max(required_stack);
    exception_table.clear();
    attributes.retain(|attribute| {
        !matches!(
            attribute,
            Attribute::StackMapTable { .. }
                | Attribute::LineNumberTable { .. }
                | Attribute::LocalVariableTable { .. }
                | Attribute::LocalVariableTypeTable { .. }
        )
    });
    *code = body;
    Ok(true)
}

fn insert_stack_map_at(
    stack_map_utf8: u16,
    attributes: &mut Vec<Attribute>,
    absolute_index: u16,
) -> Result<(), TransformError> {
    let frame = same_frame_at(absolute_index);

    if let Some(Attribute::StackMapTable { frames, .. }) = attributes
        .iter_mut()
        .find(|attribute| matches!(attribute, Attribute::StackMapTable { .. }))
    {
        insert_frame_at_absolute(frames, absolute_index, frame)?;
        return Ok(());
    }

    attributes.push(Attribute::StackMapTable {
        name_index: stack_map_utf8,
        frames: vec![frame],
    });
    Ok(())
}

fn same_frame_at(absolute_index: u16) -> StackFrame {
    if absolute_index <= 63 {
        StackFrame::SameFrame {
            frame_type: absolute_index as u8,
        }
    } else {
        StackFrame::SameFrameExtended {
            frame_type: 251,
            offset_delta: absolute_index,
        }
    }
}

fn insert_frame_at_absolute(
    frames: &mut Vec<StackFrame>,
    absolute_index: u16,
    frame: StackFrame,
) -> Result<(), TransformError> {
    if frames.is_empty() {
        frames.push(frame);
        return Ok(());
    }

    
    let mut absolutes = Vec::with_capacity(frames.len());
    let mut current = -1i32;
    for (i, existing) in frames.iter().enumerate() {
        let delta = i32::from(existing.offset_delta());
        let absolute = if i == 0 { delta } else { current + delta + 1 };
        absolutes.push(absolute);
        current = absolute;
    }

    if absolutes
        .iter()
        .any(|value| *value == i32::from(absolute_index))
    {
        return Ok(());
    }

    let insert_at = absolutes
        .iter()
        .position(|value| *value > i32::from(absolute_index))
        .unwrap_or(frames.len());

    frames.insert(insert_at, frame);
    absolutes.insert(insert_at, i32::from(absolute_index));

    
    for (i, existing) in frames.iter_mut().enumerate() {
        let absolute = u16::try_from(absolutes[i]).map_err(|_| TransformError::OffsetOverflow)?;
        let delta = if i == 0 {
            absolute
        } else {
            let prev =
                u16::try_from(absolutes[i - 1]).map_err(|_| TransformError::OffsetOverflow)?;
            absolute
                .checked_sub(prev)
                .and_then(|value| value.checked_sub(1))
                .ok_or(TransformError::OffsetOverflow)?
        };
        set_frame_offset(existing, delta)?;
    }
    Ok(())
}

fn set_frame_offset(frame: &mut StackFrame, offset_delta: u16) -> Result<(), TransformError> {
    match frame {
        StackFrame::SameFrame { .. } => {
            *frame = same_frame_at(offset_delta);
        }
        StackFrame::SameLocals1StackItemFrame { stack, .. } => {
            if offset_delta <= 63 {
                *frame = StackFrame::SameLocals1StackItemFrame {
                    frame_type: (offset_delta + 64) as u8,
                    stack: stack.clone(),
                };
            } else {
                *frame = StackFrame::SameLocals1StackItemFrameExtended {
                    frame_type: 247,
                    offset_delta,
                    stack: stack.clone(),
                };
            }
        }
        StackFrame::SameLocals1StackItemFrameExtended {
            offset_delta: slot, ..
        }
        | StackFrame::ChopFrame {
            offset_delta: slot, ..
        }
        | StackFrame::SameFrameExtended {
            offset_delta: slot, ..
        }
        | StackFrame::AppendFrame {
            offset_delta: slot, ..
        }
        | StackFrame::FullFrame {
            offset_delta: slot, ..
        } => {
            *slot = offset_delta;
        }
    }
    Ok(())
}

fn ensure_shiftable(attributes: &[Attribute]) -> Result<(), TransformError> {
    for attribute in attributes {
        match attribute {
            Attribute::RuntimeVisibleTypeAnnotations { .. } => {
                return Err(TransformError::UnsupportedAttribute(
                    "RuntimeVisibleTypeAnnotations",
                ));
            }
            Attribute::RuntimeInvisibleTypeAnnotations { .. } => {
                return Err(TransformError::UnsupportedAttribute(
                    "RuntimeInvisibleTypeAnnotations",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}







fn encoded_len(instructions: &[Instruction]) -> Result<u16, TransformError> {
    let mut total = 0usize;
    for instruction in instructions {
        let mut buffer = std::io::Cursor::new(Vec::new());
        instruction
            .to_bytes(&mut buffer)
            .map_err(|error| TransformError::ClassFile(error.to_string()))?;
        total += buffer.into_inner().len();
    }
    u16::try_from(total).map_err(|_| TransformError::OffsetOverflow)
}

fn shift_code(code: &mut [Instruction], shift: u16) -> Result<(), TransformError> {
    for instruction in code {
        let target = match instruction {
            Instruction::Ifeq(target)
            | Instruction::Ifne(target)
            | Instruction::Iflt(target)
            | Instruction::Ifge(target)
            | Instruction::Ifgt(target)
            | Instruction::Ifle(target)
            | Instruction::If_icmpeq(target)
            | Instruction::If_icmpne(target)
            | Instruction::If_icmplt(target)
            | Instruction::If_icmpge(target)
            | Instruction::If_icmpgt(target)
            | Instruction::If_icmple(target)
            | Instruction::If_acmpeq(target)
            | Instruction::If_acmpne(target)
            | Instruction::Goto(target)
            | Instruction::Jsr(target)
            | Instruction::Ifnull(target)
            | Instruction::Ifnonnull(target) => Some(target),
            _ => None,
        };
        if let Some(target) = target {
            *target = add_offset(*target, shift)?;
        }

        match instruction {
            Instruction::Goto_w(target) | Instruction::Jsr_w(target) => {
                *target = target
                    .checked_add(i32::from(shift))
                    .ok_or(TransformError::OffsetOverflow)?;
            }
            _ => {}
        }
    }
    Ok(())
}




fn shift_code_attributes(
    attributes: &mut [Attribute],
    shift: u16,
    byte_shift: u16,
) -> Result<(), TransformError> {
    for attribute in attributes {
        match attribute {
            Attribute::LineNumberTable { line_numbers, .. } => {
                for line in line_numbers {
                    line.start_pc = add_offset(line.start_pc, shift)?;
                }
            }
            
            
            
            
            
            
            
            
            
            
            
            
            
            
            
            
            Attribute::LocalVariableTable { variables, .. } => {
                for variable in variables {
                    variable.start_pc = add_offset(variable.start_pc, byte_shift)?;
                }
            }
            Attribute::LocalVariableTypeTable { variable_types, .. } => {
                for variable in variable_types {
                    variable.start_pc = add_offset(variable.start_pc, byte_shift)?;
                }
            }
            Attribute::StackMapTable { frames, .. } => {
                if let Some(first) = frames.first_mut() {
                    shift_first_frame(first, shift)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn shift_first_frame(frame: &mut StackFrame, shift: u16) -> Result<(), TransformError> {
    match frame {
        StackFrame::SameFrame { frame_type } => {
            let offset = add_offset(u16::from(*frame_type), shift)?;
            if offset <= 63 {
                *frame_type = offset as u8;
            } else {
                *frame = StackFrame::SameFrameExtended {
                    frame_type: 251,
                    offset_delta: offset,
                };
            }
        }
        StackFrame::SameLocals1StackItemFrame { frame_type, stack } => {
            let offset = add_offset(u16::from(*frame_type).saturating_sub(64), shift)?;
            if offset <= 63 {
                *frame_type = (offset + 64) as u8;
            } else {
                *frame = StackFrame::SameLocals1StackItemFrameExtended {
                    frame_type: 247,
                    offset_delta: offset,
                    stack: stack.clone(),
                };
            }
        }
        StackFrame::SameLocals1StackItemFrameExtended { offset_delta, .. }
        | StackFrame::ChopFrame { offset_delta, .. }
        | StackFrame::SameFrameExtended { offset_delta, .. }
        | StackFrame::AppendFrame { offset_delta, .. }
        | StackFrame::FullFrame { offset_delta, .. } => {
            *offset_delta = add_offset(*offset_delta, shift)?;
        }
    }
    Ok(())
}

fn callback_returns_value(descriptor: &str) -> bool {
    !descriptor.ends_with('V')
}

fn add_offset(value: u16, shift: u16) -> Result<u16, TransformError> {
    value
        .checked_add(shift)
        .ok_or(TransformError::OffsetOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ristretto_classfile::{
        ClassAccessFlags, ConstantPool, JAVA_17, Method, MethodAccessFlags,
        attributes::{Attribute, StackFrame},
    };
    use std::sync::{Mutex, OnceLock};

    fn test_gate() -> &'static Mutex<()> {
        crate::native_jvmti::hook_registry_test_lock()
    }

    fn fixture_class() -> Vec<u8> {
        let mut constant_pool = ConstantPool::new();
        let this_class = constant_pool.add_class("fixture/Target").unwrap();
        let super_class = constant_pool.add_class("java/lang/Object").unwrap();
        let method_name = constant_pool.add_utf8("tick").unwrap();
        let descriptor = constant_pool.add_utf8("()V").unwrap();
        let code_name = constant_pool.add_utf8("Code").unwrap();
        let class_file = ClassFile {
            version: JAVA_17,
            constant_pool,
            access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
            this_class,
            super_class,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: vec![Method {
                access_flags: MethodAccessFlags::PUBLIC,
                name_index: method_name,
                descriptor_index: descriptor,
                attributes: vec![Attribute::Code {
                    name_index: code_name,
                    max_stack: 0,
                    max_locals: 1,
                    code: vec![Instruction::Return],
                    exception_table: Vec::new(),
                    attributes: Vec::new(),
                }],
            }],
            attributes: Vec::new(),
            code_source_url: None,
        };
        let mut bytes = Vec::new();
        class_file.to_bytes(&mut bytes).unwrap();
        bytes
    }

    
    
    fn fixture_class_with_four_locals() -> Vec<u8> {
        let mut constant_pool = ConstantPool::new();
        let this_class = constant_pool.add_class("fixture/Target").unwrap();
        let super_class = constant_pool.add_class("java/lang/Object").unwrap();
        let method_name = constant_pool.add_utf8("tick").unwrap();
        let descriptor = constant_pool
            .add_utf8("(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)V")
            .unwrap();
        let code_name = constant_pool.add_utf8("Code").unwrap();
        let class_file = ClassFile {
            version: JAVA_17,
            constant_pool,
            access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
            this_class,
            super_class,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: vec![Method {
                access_flags: MethodAccessFlags::PUBLIC,
                name_index: method_name,
                descriptor_index: descriptor,
                attributes: vec![Attribute::Code {
                    name_index: code_name,
                    max_stack: 0,
                    max_locals: 4,
                    code: vec![Instruction::Return],
                    exception_table: Vec::new(),
                    attributes: Vec::new(),
                }],
            }],
            attributes: Vec::new(),
            code_source_url: None,
        };
        let mut bytes = Vec::new();
        class_file.to_bytes(&mut bytes).unwrap();
        bytes
    }

    fn fixture_class_with_branch() -> Vec<u8> {
        let mut constant_pool = ConstantPool::new();
        let this_class = constant_pool.add_class("fixture/Target").unwrap();
        let super_class = constant_pool.add_class("java/lang/Object").unwrap();
        let method_name = constant_pool.add_utf8("tick").unwrap();
        let descriptor = constant_pool.add_utf8("()V").unwrap();
        let code_name = constant_pool.add_utf8("Code").unwrap();
        let stack_map = constant_pool.add_utf8("StackMapTable").unwrap();
        let class_file = ClassFile {
            version: JAVA_17,
            constant_pool,
            access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
            this_class,
            super_class,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: vec![Method {
                access_flags: MethodAccessFlags::PUBLIC,
                name_index: method_name,
                descriptor_index: descriptor,
                attributes: vec![Attribute::Code {
                    name_index: code_name,
                    max_stack: 1,
                    max_locals: 1,
                    
                    code: vec![
                        Instruction::Iconst_0,
                        Instruction::Ifne(3),
                        Instruction::Return,
                        Instruction::Return,
                    ],
                    exception_table: Vec::new(),
                    attributes: vec![Attribute::StackMapTable {
                        name_index: stack_map,
                        frames: vec![StackFrame::SameFrame { frame_type: 3 }],
                    }],
                }],
            }],
            attributes: Vec::new(),
            code_source_url: None,
        };
        let mut bytes = Vec::new();
        class_file.to_bytes(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn prepends_native_entry_callback_and_round_trips() {
        let _lock = test_gate().lock().unwrap_or_else(|p| p.into_inner());
        clear_hooks();
        register_hook(HookSpec::entry(
            ["fixture/Target"],
            "tick",
            "()V",
            "onTick",
            "()V",
            Vec::new(),
        ));

        let transformed = transform("fixture/Target", &fixture_class())
            .unwrap()
            .expect("class should be transformed");
        let parsed = ClassFile::from_bytes(&transformed).unwrap();
        let Attribute::Code { code, .. } = &parsed.methods[0].attributes[0] else {
            panic!("missing code");
        };
        assert!(matches!(code[0], Instruction::Invokestatic(_)));
        assert!(matches!(code[1], Instruction::Return));
        clear_hooks();
    }

    #[test]
    fn entry_hook_pops_boolean_return() {
        let _lock = test_gate().lock().unwrap_or_else(|p| p.into_inner());
        clear_hooks();
        register_hook(HookSpec::entry(
            ["fixture/Target"],
            "tick",
            "()V",
            "onLivingSubmit",
            "(Ljava/lang/Object;)Z",
            vec![LocalLoad::Reference(0)],
        ));

        let transformed = transform("fixture/Target", &fixture_class())
            .unwrap()
            .expect("class should be transformed");
        let parsed = ClassFile::from_bytes(&transformed).unwrap();
        let Attribute::Code { code, .. } = &parsed.methods[0].attributes[0] else {
            panic!("missing code");
        };
        assert!(matches!(code[0], Instruction::Aload_0), "got {code:?}");
        assert!(
            matches!(code[1], Instruction::Invokestatic(_)),
            "got {code:?}"
        );
        assert!(matches!(code[2], Instruction::Pop), "got {code:?}");
        assert!(matches!(code[3], Instruction::Return), "got {code:?}");
        clear_hooks();
    }

    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    const MIXIN_DISPATCHER: &[u8] =
        include_bytes!("../../../tests/fixtures/mixin_EntityRenderDispatcher.class");

    
    
    
    
    
    
    
    
    
    #[test]
    fn local_variable_tables_shift_by_bytes_not_instructions() {
        let _lock = test_gate().lock().unwrap_or_else(|p| p.into_inner());
        clear_hooks();
        
        
        let symbols = crate::native_jvmti::RENDER_SYMBOLS;
        let spec = HookSpec::entry(
            [symbols.entity_render_dispatcher],
            symbols.dispatcher_submit.name,
            symbols.dispatcher_submit.descriptor,
            "onSubmitWorld",
            "(Ljava/lang/Object;Ljava/lang/Object;)V",
            vec![LocalLoad::Reference(9), LocalLoad::Reference(10)],
        );
        let target = spec.class_names[0].clone();
        register_hook(spec.clone());

        let before = ClassFile::from_bytes(MIXIN_DISPATCHER).expect("parse fixture");
        let transformed = transform(&target, MIXIN_DISPATCHER)
            .expect("transform must not error")
            .expect("must transform");
        let after = ClassFile::from_bytes(&transformed).expect("re-parse");

        let starts = |class: &ClassFile<'_>| -> Vec<u16> {
            for method in &class.methods {
                let name = class
                    .constant_pool
                    .try_get_utf8(method.name_index)
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                let descriptor = class
                    .constant_pool
                    .try_get_utf8(method.descriptor_index)
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                if name != spec.method_name || descriptor != spec.method_descriptor {
                    continue;
                }
                for attribute in &method.attributes {
                    if let Attribute::Code { attributes, .. } = attribute {
                        for inner in attributes {
                            if let Attribute::LocalVariableTable { variables, .. } = inner {
                                return variables.iter().map(|entry| entry.start_pc).collect();
                            }
                        }
                    }
                }
            }
            Vec::new()
        };

        let original = starts(&before);
        let shifted = starts(&after);
        assert!(!original.is_empty(), "fixture must have a LocalVariableTable");
        assert_eq!(original.len(), shifted.len());

        
        let expected = encoded_len(
            &spec
                .loads
                .iter()
                .copied()
                .map(LocalLoad::instruction)
                .chain(std::iter::once(Instruction::Invokestatic(0)))
                .collect::<Vec<_>>(),
        )
        .expect("prologue size");
        assert_eq!(expected, 7, "prologue is 7 bytes for this spec");
        assert_ne!(
            expected,
            u16::try_from(spec.loads.len() + 1).unwrap(),
            "byte length must differ from instruction count, else the test proves nothing"
        );

        for (original, shifted) in original.iter().zip(shifted.iter()) {
            assert_eq!(
                *shifted,
                original + expected,
                "start_pc must move by the prologue's BYTE length"
            );
        }
        clear_hooks();
    }

    
    
    
    
    
    #[test]
    fn line_emitter_packs_a_real_direction_per_segment() {
        let source = include_str!("../../avatar/render/MeshEmit.java");
        let body = source
            .split("public static void emitLines(")
            .nth(1)
            .expect("emitLines exists");
        let body = body.split("private MeshEmit").next().unwrap_or(body);
        assert!(
            !body.contains("setNormal(0.0f, 1.0f, 0.0f)"),
            "a constant normal makes every non-vertical line invisible"
        );
        assert!(
            body.contains("setNormal(pose,"),
            "direction must be transformed by the Pose, like the position"
        );
        assert!(
            body.contains("o += 8"),
            "layout must carry the 3 direction floats (8 per vertex)"
        );
    }

    #[test]
    fn world_hook_transforms_its_real_target() {
        use std::io::Read;

        let Ok(path) = std::env::var("XZ_MC_26_2_CLIENT_JAR") else {
            return;
        };
        let _lock = test_gate().lock().unwrap_or_else(|p| p.into_inner());
        clear_hooks();

        let spec = crate::world_render::world_hook_spec();
        let target = spec.class_names[0].clone();
        register_hook(spec.clone());

        let file = std::fs::File::open(path).expect("open client fixture");
        let mut archive = zip::ZipArchive::new(file).expect("open jar");
        let mut entry = archive
            .by_name(&format!("{target}.class"))
            .expect("hook target in jar");
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("read class");

        let transformed = transform(&target, &bytes)
            .expect("transform must not error")
            .expect("target must actually be transformed");
        let parsed = ClassFile::from_bytes(&transformed).expect("must re-parse");
        parsed.verify().expect("must verify");

        let method = parsed
            .methods
            .iter()
            .find(|method| {
                parsed
                    .constant_pool
                    .try_get_utf8(method.name_index)
                    .is_ok_and(|name| name.to_string() == spec.method_name)
                    && parsed
                        .constant_pool
                        .try_get_utf8(method.descriptor_index)
                        .is_ok_and(|descriptor| descriptor.to_string() == spec.method_descriptor)
            })
            .expect("hooked method survived");
        let Some(Attribute::Code {
            code, max_locals, ..
        }) = method
            .attributes
            .iter()
            .find(|attribute| matches!(attribute, Attribute::Code { .. }))
        else {
            panic!("hooked method lost its Code attribute");
        };

        
        for (index, load) in spec.loads.iter().enumerate() {
            assert_eq!(
                code[index],
                load.instruction(),
                "prologue instruction {index} must match the spec"
            );
        }
        assert!(
            matches!(code[spec.loads.len()], Instruction::Invokestatic(_)),
            "prologue must end in the trampoline call"
        );
        
        for load in &spec.loads {
            let slot = match load {
                LocalLoad::Reference(slot)
                | LocalLoad::Int(slot)
                | LocalLoad::Float(slot)
                | LocalLoad::Long(slot)
                | LocalLoad::Double(slot) => u16::from(*slot),
            };
            assert!(
                slot < *max_locals,
                "slot {slot} is outside max_locals={max_locals}: wrong parameter index"
            );
        }
        clear_hooks();
    }

    #[test]
    fn world_hook_loads_pose_stack_and_collector_slots() {
        let _lock = test_gate().lock().unwrap_or_else(|p| p.into_inner());
        clear_hooks();
        register_hook(HookSpec::entry(
            ["fixture/Target"],
            "tick",
            "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)V",
            "onSubmitWorld",
            "(Ljava/lang/Object;Ljava/lang/Object;)V",
            vec![LocalLoad::Reference(1), LocalLoad::Reference(3)],
        ));

        let transformed = transform("fixture/Target", &fixture_class_with_four_locals())
            .unwrap()
            .expect("class should be transformed");
        let parsed = ClassFile::from_bytes(&transformed).unwrap();
        let Attribute::Code {
            code, max_locals, ..
        } = &parsed.methods[0].attributes[0]
        else {
            panic!("missing code");
        };
        assert!(matches!(code[0], Instruction::Aload_1), "got {code:?}");
        assert!(matches!(code[1], Instruction::Aload_3), "got {code:?}");
        assert!(
            matches!(code[2], Instruction::Invokestatic(_)),
            "got {code:?}"
        );
        
        assert!(matches!(code[3], Instruction::Return), "got {code:?}");
        assert!(
            *max_locals >= 4,
            "slot 3 must be addressable, got max_locals={max_locals}"
        );
        clear_hooks();
    }

    #[test]
    fn gate_hook_uses_ifne_epilogue_and_verifies() {
        let _lock = test_gate().lock().unwrap_or_else(|p| p.into_inner());
        clear_hooks();
        register_hook(HookSpec::gate(
            ["fixture/Target"],
            "tick",
            "()V",
            "onLivingSubmit",
            "(Ljava/lang/Object;)Z",
            vec![LocalLoad::Reference(0)],
        ));

        let transformed = transform("fixture/Target", &fixture_class_with_branch())
            .unwrap()
            .expect("class should be transformed");
        let parsed = ClassFile::from_bytes(&transformed).unwrap();
        parsed.verify().expect("transformed class must verify");
        let Attribute::Code { code, .. } = &parsed.methods[0].attributes[0] else {
            panic!("missing code");
        };
        assert!(matches!(code[0], Instruction::Aload_0), "got {code:?}");
        assert!(
            matches!(code[1], Instruction::Invokestatic(_)),
            "got {code:?}"
        );
        assert!(matches!(code[2], Instruction::Ifne(_)), "got {code:?}");
        assert!(matches!(code.last(), Some(Instruction::Return)));
        clear_hooks();
    }
}
