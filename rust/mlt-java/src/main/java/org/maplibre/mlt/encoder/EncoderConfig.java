package org.maplibre.mlt.encoder;

import java.lang.foreign.MemorySegment;
import lombok.Builder;
import org.jetbrains.annotations.NotNull;
import org.maplibre.mlt.encoder.ffi.mlt_ffi_h;

/// Configuration for the MLT encoder, controlling optimization strategies.
@Builder
public final class EncoderConfig {
  @Builder.Default private final boolean tessellate = false;
  @Builder.Default private final boolean trySpatialMortonSort = true;
  @Builder.Default private final boolean trySpatialHilbertSort = true;
  @Builder.Default private final boolean tryIdSort = true;
  @Builder.Default private final boolean allowFsst = true;
  @Builder.Default private final boolean allowFastPfor = true;
  @Builder.Default private final boolean allowSharedDict = true;

  public static @NotNull EncoderConfig defaults() {
    return builder().build();
  }

  public boolean tessellate() {
    return tessellate;
  }

  public boolean tryMortonSort() {
    return trySpatialMortonSort;
  }

  public boolean tryHilbertSort() {
    return trySpatialHilbertSort;
  }

  public boolean tryIdSort() {
    return tryIdSort;
  }

  public boolean allowFsst() {
    return allowFsst;
  }

  public boolean allowFastPfor() {
    return allowFastPfor;
  }

  public boolean allowSharedDict() {
    return allowSharedDict;
  }

  /// Create an opaque native MltEncoderConfig via diplomat setters.
  /// The caller MUST call MltEncoderConfig_destroy on the returned pointer when done.
  MemorySegment toNative() {
    MemorySegment ptr = mlt_ffi_h.MltEncoderConfig_new_default();
    mlt_ffi_h.MltEncoderConfig_set_tessellate(ptr, tessellate);
    mlt_ffi_h.MltEncoderConfig_set_try_morton_sort(ptr, trySpatialMortonSort);
    mlt_ffi_h.MltEncoderConfig_set_try_hilbert_sort(ptr, trySpatialHilbertSort);
    mlt_ffi_h.MltEncoderConfig_set_try_id_sort(ptr, tryIdSort);
    mlt_ffi_h.MltEncoderConfig_set_allow_fsst(ptr, allowFsst);
    mlt_ffi_h.MltEncoderConfig_set_allow_fast_pfor(ptr, allowFastPfor);
    mlt_ffi_h.MltEncoderConfig_set_allow_shared_dict(ptr, allowSharedDict);
    return ptr;
  }
}
