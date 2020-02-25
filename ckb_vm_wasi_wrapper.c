#include "helloworld_riscv_glue.h"
#include "ckb_syscalls.h"
#include <stdio.h>

int32_t wavm_wasi_unstable_fd_write(void* dummy, int32_t fd, int32_t address, int32_t num, int32_t writtenBytesAddress)
{
  static uint8_t temp_buffer[65];

  int32_t written_bytes = 0;
  for (int32_t i = 0; i < num; i++) {
    uint32_t buffer_address = *((uint32_t*) &memory0[address + i * 8]);
    uint8_t* buf = &memory0[buffer_address];
    uint32_t buffer_length = *((uint32_t*) &memory0[address + i * 8 + 4]);

    int32_t written = 0;
    while (written < buffer_length) {
      int32_t left_bytes = buffer_length - written;
      int32_t b = (left_bytes > 64) ? 64 : left_bytes;
      memcpy(temp_buffer, &buf[written], b);
      temp_buffer[b] = '\0';
      ckb_debug(temp_buffer);

      written += b;
    }

    written_bytes += buffer_length;
  }

  return 0;
}

void wavm_wasi_unstable_proc_exit(void* dummy, int32_t code)
{
  ckb_exit(code);
}
