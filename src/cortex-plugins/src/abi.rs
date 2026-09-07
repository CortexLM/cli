//! WASM protocol 1: `input_len()`, `read_input(ptr, cap)`, `set_result(ptr, len)`.
//! Exports are `() -> i32`: zero is success, any other status is an error.
use crate::host::HasHostState;
use crate::{PluginError, Result};
use wasmtime::{Caller, Linker};

pub fn register<T: HasHostState + 'static>(linker: &mut Linker<T>) -> Result<()> {
    linker
        .func_wrap("cortex", "input_len", |caller: Caller<'_, T>| -> i32 {
            caller.data().host_state().input.len() as i32
        })
        .map_err(|e| PluginError::WasmError(e.to_string()))?;
    linker
        .func_wrap(
            "cortex",
            "read_input",
            |mut caller: Caller<'_, T>, ptr: i32, cap: i32| -> i32 {
                let input = caller.data().host_state().input.clone();
                if ptr < 0 || cap < input.len() as i32 {
                    return -1;
                }
                let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                    return -1;
                };
                match memory.write(&mut caller, ptr as usize, &input) {
                    Ok(()) => input.len() as i32,
                    Err(_) => -1,
                }
            },
        )
        .map_err(|e| PluginError::WasmError(e.to_string()))?;
    linker
        .func_wrap(
            "cortex",
            "set_result",
            |mut caller: Caller<'_, T>, ptr: i32, len: i32| -> i32 {
                if ptr < 0 || len < 0 || len as usize > crate::contract::MAX_FRAME_BYTES {
                    return -1;
                }
                let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                    return -1;
                };
                let mut bytes = vec![0; len as usize];
                if memory.read(&caller, ptr as usize, &mut bytes).is_err() {
                    return -1;
                }
                let Ok(result) = serde_json::from_slice(&bytes) else {
                    return -2;
                };
                caller.data_mut().host_state_mut().result = Some(result);
                0
            },
        )
        .map_err(|e| PluginError::WasmError(e.to_string()))?;
    Ok(())
}
