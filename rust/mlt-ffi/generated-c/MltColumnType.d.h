#ifndef MltColumnType_D_H
#define MltColumnType_D_H

#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include "diplomat_runtime.h"





typedef enum MltColumnType {
  MltColumnType_Bool = 0,
  MltColumnType_I8 = 1,
  MltColumnType_U8 = 2,
  MltColumnType_I32 = 3,
  MltColumnType_U32 = 4,
  MltColumnType_I64 = 5,
  MltColumnType_U64 = 6,
  MltColumnType_F32 = 7,
  MltColumnType_F64 = 8,
} MltColumnType;

typedef struct MltColumnType_option {union { MltColumnType ok; }; bool is_ok; } MltColumnType_option;



#endif // MltColumnType_D_H
