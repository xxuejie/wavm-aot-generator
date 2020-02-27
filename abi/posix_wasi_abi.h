#include <stdlib.h>
#include <unistd.h>

#ifndef WAVM_POSIX_WASI_ABI_H
#define WAVM_POSIX_WASI_ABI_H

#ifndef MEMORY0_DEFINED
extern uint8_t* memory0;
#endif

int32_t wavm_wasi_unstable_fd_write(void* dummy, int32_t fd, int32_t address, int32_t num, int32_t written_bytes_address)
{
  (void) dummy;

  int32_t written_bytes = 0;
  for (int32_t i = 0; i < num; i++) {
    uint32_t buffer_address = *((uint32_t*) &memory0[address + i * 8]);
    uint8_t* buf = &memory0[buffer_address];
    uint32_t buffer_length = *((uint32_t*) &memory0[address + i * 8 + 4]);

    int32_t written = write(fd, buf, buffer_length);
    written_bytes += written;
  }
  if (written_bytes_address != 0) {
    *((uint32_t*) &memory0[written_bytes_address]) = written_bytes;
  }

  return 0;
}

void wavm_wasi_unstable_proc_exit(void* dummy, int32_t code)
{
  exit(code);
}

#endif  /* WAVM_POSIX_WASI_ABI_H */
