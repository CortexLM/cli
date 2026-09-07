;; Known-source test module. Wasmtime compiles this WAT before executing it.
(module
  (import "cortex" "input_len" (func $input_len (result i32)))
  (import "cortex" "read_input" (func $read_input (param i32 i32) (result i32)))
  (import "cortex" "set_result" (func $set_result (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (global $counter (mut i32) (i32.const 0))
  (data (i32.const 0) "{\"data\":")
  (data (i32.const 2000) "{\"data\":0}")
  (data (i32.const 4000) "{\"data\":-1}")
  (func (export "init") (result i32) i32.const 0)
  (func (export "shutdown") (result i32) i32.const 0)
  (func (export "cmd_echo") (result i32)
    i32.const 1024 i32.const 60000 call $read_input drop
    i32.const 8 i32.const 1024 call $input_len memory.copy
    call $input_len i32.const 8 i32.add i32.const 125 i32.store8
    i32.const 0 call $input_len i32.const 9 i32.add call $set_result)
  (func (export "cmd_count") (result i32)
    global.get $counter i32.const 1 i32.add global.set $counter
    i32.const 2008 global.get $counter i32.const 48 i32.add i32.store8
    i32.const 2000 i32.const 10 call $set_result)
  (func (export "cmd_loop") (result i32)
    (loop $forever br $forever) i32.const 0)
  (func (export "cmd_bounds") (result i32)
    i32.const 65535 i32.const 32 call $set_result)
  (func (export "cmd_memory") (result i32)
    i32.const 257 memory.grow i32.const -1 i32.eq
    if (result i32)
      i32.const 4000 i32.const 11 call $set_result
    else
      i32.const 7
    end)
)
