#ifndef MltTileEncoder_H
#define MltTileEncoder_H

#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include "diplomat_runtime.h"

#include "MltEncodedBuffer.d.h"
#include "MltEncoderConfig.d.h"
#include "MltError.d.h"
#include "MltLayerEncoder.d.h"

#include "MltTileEncoder.d.h"






MltTileEncoder* MltTileEncoder_new(void);

typedef struct MltTileEncoder_add_layer_result {union { MltError* err;}; bool is_ok;} MltTileEncoder_add_layer_result;
MltTileEncoder_add_layer_result MltTileEncoder_add_layer(MltTileEncoder* self, MltLayerEncoder* layer);

typedef struct MltTileEncoder_encode_result {union {MltEncodedBuffer* ok; MltError* err;}; bool is_ok;} MltTileEncoder_encode_result;
MltTileEncoder_encode_result MltTileEncoder_encode(MltTileEncoder* self, const MltEncoderConfig* config);

void MltTileEncoder_destroy(MltTileEncoder* self);





#endif // MltTileEncoder_H
