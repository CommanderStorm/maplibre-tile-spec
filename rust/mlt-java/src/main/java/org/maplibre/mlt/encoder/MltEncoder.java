package org.maplibre.mlt.encoder;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.util.List;
import org.jetbrains.annotations.NotNull;
import org.maplibre.mlt.encoder.ffi.DiplomatBoolView;
import org.maplibre.mlt.encoder.ffi.DiplomatU64View;
import org.maplibre.mlt.encoder.ffi.DiplomatU8View;
import org.maplibre.mlt.encoder.ffi.MltTileEncoder_encode_result;
import org.maplibre.mlt.encoder.ffi.mlt_ffi_h;

/// Main API for encoding tile data using the Rust MLT encoder via Panama FFM.
public final class MltEncoder {

  private static final boolean NATIVE_AVAILABLE;

  static {
    boolean loaded;
    try {
      NativeLoader.load();
      Class.forName("org.maplibre.mlt.encoder.ffi.mlt_ffi_h");
      loaded = true;
    } catch (UnsatisfiedLinkError | ClassNotFoundException e) {
      loaded = false;
    }
    NATIVE_AVAILABLE = loaded;
  }

  private MltEncoder() {}

  public static boolean isAvailable() {
    return NATIVE_AVAILABLE;
  }

  public static byte @NotNull [] encode(@NotNull List<@NotNull Layer> layers) {
    return encode(layers, EncoderConfig.defaults());
  }

  public static byte @NotNull [] encode(
      @NotNull List<@NotNull Layer> layers, @NotNull EncoderConfig config) {
    try (Arena arena = Arena.ofConfined()) {
      MemorySegment tilePtr = mlt_ffi_h.MltTileEncoder_new();
      if (tilePtr == MemorySegment.NULL) {
        throw new MltEncodeException(
            MltEncodeException.Phase.TILE_CREATE, null, null, "MltTileEncoder_new returned null");
      }
      boolean tileConsumed = false;
      try {
        MemorySegment configPtr = config.toNative();
        if (configPtr == MemorySegment.NULL) {
          throw new MltEncodeException(
              MltEncodeException.Phase.INIT, null, null, "MltEncoderConfig_new returned null");
        }
        try {
          for (Layer layer : layers) {
            MemorySegment layerPtr = FfiHelpers.newLayer(layer.name(), layer.extent(), arena);
            boolean layerConsumed = false;
            try {
              List<Feature> features = layer.features();
              int featureCount = features.size();
              setIds(layerPtr, features, featureCount, arena);
              GeometryColumnizer.write(layerPtr, layer, features, featureCount, arena);
              PropertyColumnWriter.writeColumns(layerPtr, layer, features, featureCount, arena);

              MemorySegment addResult =
                  mlt_ffi_h.MltTileEncoder_add_layer(arena, tilePtr, layerPtr);
              layerConsumed = true;
              FfiHelpers.checkResult(
                  addResult, MltEncodeException.Phase.TILE_ADD_LAYER, layer.name(), null);
            } finally {
              if (!layerConsumed) {
                mlt_ffi_h.MltLayerEncoder_destroy(layerPtr);
              }
            }
          }

          MemorySegment encodeResult = mlt_ffi_h.MltTileEncoder_encode(arena, tilePtr, configPtr);
          tileConsumed = true;
          if (!MltTileEncoder_encode_result.is_ok(encodeResult)) {
            MemorySegment errPtr = MltTileEncoder_encode_result.err(encodeResult);
            FfiHelpers.readErrorAndThrow(errPtr, MltEncodeException.Phase.TILE_ENCODE, null, null);
          }
          MemorySegment encodedBufPtr = MltTileEncoder_encode_result.ok(encodeResult);
          try {
            MemorySegment bytesView = mlt_ffi_h.MltEncodedBuffer_as_bytes(arena, encodedBufPtr);
            MemorySegment dataPtr = DiplomatU8View.data(bytesView);
            long len = DiplomatU8View.len(bytesView);
            return dataPtr.reinterpret(len).toArray(ValueLayout.JAVA_BYTE);
          } finally {
            mlt_ffi_h.MltEncodedBuffer_destroy(encodedBufPtr);
          }
        } finally {
          mlt_ffi_h.MltEncoderConfig_destroy(configPtr);
        }
      } finally {
        if (!tileConsumed) {
          mlt_ffi_h.MltTileEncoder_destroy(tilePtr);
        }
      }
    }
  }

  private static void setIds(
      MemorySegment layerPtr, List<Feature> features, int featureCount, Arena arena) {
    boolean anyId = false;
    for (Feature f : features) {
      if (f.hasId()) {
        anyId = true;
        break;
      }
    }
    if (!anyId) {
      return;
    }

    MemorySegment ids = arena.allocate((long) featureCount * 8);
    MemorySegment present = arena.allocate(featureCount);
    for (int i = 0; i < featureCount; i++) {
      Feature f = features.get(i);
      if (f.hasId()) {
        ids.set(mlt_ffi_h.C_LONG_LONG, (long) i * 8, f.id().getAsLong());
        present.set(mlt_ffi_h.C_BOOL, i, true);
      } else {
        ids.set(mlt_ffi_h.C_LONG_LONG, (long) i * 8, 0L);
        present.set(mlt_ffi_h.C_BOOL, i, false);
      }
    }

    MemorySegment idsView = FfiHelpers.makeView(ids, featureCount, DiplomatU64View.layout(), arena);
    MemorySegment presentView =
        FfiHelpers.makeView(present, featureCount, DiplomatBoolView.layout(), arena);
    MemorySegment result = mlt_ffi_h.MltLayerEncoder_set_ids(arena, layerPtr, idsView, presentView);
    FfiHelpers.checkResult(result, MltEncodeException.Phase.SET_IDS, null, null);
  }
}
