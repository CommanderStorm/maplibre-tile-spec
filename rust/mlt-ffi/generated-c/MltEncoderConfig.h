#ifndef MltEncoderConfig_H
#define MltEncoderConfig_H

#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include "diplomat_runtime.h"


#include "MltEncoderConfig.d.h"






MltEncoderConfig* MltEncoderConfig_new_default(void);

void MltEncoderConfig_set_tessellate(MltEncoderConfig* self, bool value);

void MltEncoderConfig_set_try_morton_sort(MltEncoderConfig* self, bool value);

void MltEncoderConfig_set_try_hilbert_sort(MltEncoderConfig* self, bool value);

void MltEncoderConfig_set_try_id_sort(MltEncoderConfig* self, bool value);

void MltEncoderConfig_set_allow_fsst(MltEncoderConfig* self, bool value);

void MltEncoderConfig_set_allow_fast_pfor(MltEncoderConfig* self, bool value);

void MltEncoderConfig_set_allow_shared_dict(MltEncoderConfig* self, bool value);

void MltEncoderConfig_destroy(MltEncoderConfig* self);





#endif // MltEncoderConfig_H
