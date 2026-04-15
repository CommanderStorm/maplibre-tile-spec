#ifndef MltEncodedBuffer_H
#define MltEncodedBuffer_H

#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include "diplomat_runtime.h"


#include "MltEncodedBuffer.d.h"






DiplomatU8View MltEncodedBuffer_as_bytes(const MltEncodedBuffer* self);

size_t MltEncodedBuffer_len(const MltEncodedBuffer* self);

bool MltEncodedBuffer_is_empty(const MltEncodedBuffer* self);

void MltEncodedBuffer_destroy(MltEncodedBuffer* self);





#endif // MltEncodedBuffer_H
