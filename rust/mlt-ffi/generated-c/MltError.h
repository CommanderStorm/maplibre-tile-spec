#ifndef MltError_H
#define MltError_H

#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include "diplomat_runtime.h"


#include "MltError.d.h"






void MltError_message(const MltError* self, DiplomatWrite* write);

void MltError_destroy(MltError* self);





#endif // MltError_H
