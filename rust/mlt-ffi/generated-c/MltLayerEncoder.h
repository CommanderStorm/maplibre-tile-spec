#ifndef MltLayerEncoder_H
#define MltLayerEncoder_H

#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include "diplomat_runtime.h"

#include "MltColumnType.d.h"
#include "MltEncodedBuffer.d.h"
#include "MltEncoderConfig.d.h"
#include "MltError.d.h"

#include "MltLayerEncoder.d.h"






typedef struct MltLayerEncoder_new_result {union {MltLayerEncoder* ok; MltError* err;}; bool is_ok;} MltLayerEncoder_new_result;
MltLayerEncoder_new_result MltLayerEncoder_new(DiplomatStringView name, uint32_t extent);

typedef struct MltLayerEncoder_set_ids_result {union { MltError* err;}; bool is_ok;} MltLayerEncoder_set_ids_result;
MltLayerEncoder_set_ids_result MltLayerEncoder_set_ids(MltLayerEncoder* self, DiplomatU64View ids, DiplomatBoolView present);

typedef struct MltLayerEncoder_set_geometries_result {union { MltError* err;}; bool is_ok;} MltLayerEncoder_set_geometries_result;
MltLayerEncoder_set_geometries_result MltLayerEncoder_set_geometries(MltLayerEncoder* self, DiplomatI32View coords, DiplomatU32View meta);

typedef struct MltLayerEncoder_add_column_result {union { MltError* err;}; bool is_ok;} MltLayerEncoder_add_column_result;
MltLayerEncoder_add_column_result MltLayerEncoder_add_column(MltLayerEncoder* self, DiplomatStringView name, MltColumnType column_type, DiplomatU8View data, uint32_t count, DiplomatBoolView present);

typedef struct MltLayerEncoder_add_string_column_result {union { MltError* err;}; bool is_ok;} MltLayerEncoder_add_string_column_result;
MltLayerEncoder_add_string_column_result MltLayerEncoder_add_string_column(MltLayerEncoder* self, DiplomatStringView name, DiplomatStringView data, DiplomatU32View offsets, DiplomatBoolView present);

typedef struct MltLayerEncoder_encode_result {union {MltEncodedBuffer* ok; MltError* err;}; bool is_ok;} MltLayerEncoder_encode_result;
MltLayerEncoder_encode_result MltLayerEncoder_encode(MltLayerEncoder* self, const MltEncoderConfig* config);

void MltLayerEncoder_destroy(MltLayerEncoder* self);





#endif // MltLayerEncoder_H
