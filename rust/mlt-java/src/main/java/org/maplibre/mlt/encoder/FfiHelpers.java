package org.maplibre.mlt.encoder;

import java.lang.foreign.Arena;
import java.lang.foreign.GroupLayout;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.charset.StandardCharsets;
import org.jetbrains.annotations.Nullable;
import org.maplibre.mlt.encoder.ffi.DiplomatStringView;
import org.maplibre.mlt.encoder.ffi.MltLayerEncoder_add_column_result;
import org.maplibre.mlt.encoder.ffi.MltLayerEncoder_new_result;
import org.maplibre.mlt.encoder.ffi.mlt_ffi_h;

/// Package-private FFI utility methods shared across encoder classes.
final class FfiHelpers {

  private FfiHelpers() {}

  /// Create a new native layer encoder, throwing on failure.
  static MemorySegment newLayer(String name, int extent, Arena arena) {
    MemorySegment nameView = makeStringView(name, arena);
    MemorySegment result = mlt_ffi_h.MltLayerEncoder_new(arena, nameView, extent);
    if (!MltLayerEncoder_new_result.is_ok(result)) {
      MemorySegment errPtr = MltLayerEncoder_new_result.err(result);
      readErrorAndThrow(errPtr, MltEncodeException.Phase.LAYER_CREATE, name, null);
    }
    return MltLayerEncoder_new_result.ok(result);
  }

  /// Create a DiplomatStringView for the given Java string, allocated in the arena.
  /// The view references UTF-8 bytes that live in the arena (NOT null-terminated).
  static MemorySegment makeStringView(String s, Arena arena) {
    byte[] bytes = s.getBytes(StandardCharsets.UTF_8);
    MemorySegment dataSeg = arena.allocate(bytes.length);
    MemorySegment.copy(bytes, 0, dataSeg, ValueLayout.JAVA_BYTE, 0, bytes.length);
    MemorySegment view = DiplomatStringView.allocate(arena);
    DiplomatStringView.data(view, dataSeg);
    DiplomatStringView.len(view, bytes.length);
    return view;
  }

  /// Create a diplomat view struct with the given data pointer and length.
  static MemorySegment makeView(MemorySegment data, long len, GroupLayout viewLayout, Arena arena) {
    MemorySegment view = arena.allocate(viewLayout);
    // All diplomat view structs have the same {pointer, size_t} layout.
    view.set(mlt_ffi_h.C_POINTER, 0, data);
    view.set(mlt_ffi_h.C_LONG, 8, len);
    return view;
  }


  // All void-result structs share the layout {union{err*}, bool is_ok, padding}.
  // Use the generated accessor offsets instead of magic constants.
  private static final long VOID_RESULT_IS_OK_OFFSET =
      MltLayerEncoder_add_column_result.is_ok$offset();
  private static final long VOID_RESULT_ERR_OFFSET = MltLayerEncoder_add_column_result.err$offset();

  /// Check a void-result struct (has only err + is_ok). If failed, extract error and throw.
  static void checkResult(
      MemorySegment resultSeg,
      MltEncodeException.Phase phase,
      @Nullable String layerName,
      @Nullable String colName) {
    boolean ok = resultSeg.get(ValueLayout.JAVA_BOOLEAN, VOID_RESULT_IS_OK_OFFSET);
    if (!ok) {
      MemorySegment errPtr = resultSeg.get(mlt_ffi_h.C_POINTER, VOID_RESULT_ERR_OFFSET);
      readErrorAndThrow(errPtr, phase, layerName, colName);
    }
  }

  /// Read an MltError message via DiplomatWrite, destroy the error, and throw MltEncodeException.
  static void readErrorAndThrow(
      MemorySegment errPtr,
      MltEncodeException.Phase phase,
      @Nullable String layerName,
      @Nullable String colName) {
    String msg;
    try {
      MemorySegment write = mlt_ffi_h.diplomat_buffer_write_create(256);
      try {
        mlt_ffi_h.MltError_message(errPtr, write);
        long len = mlt_ffi_h.diplomat_buffer_write_len(write);
        MemorySegment bytes = mlt_ffi_h.diplomat_buffer_write_get_bytes(write).reinterpret(len);
        msg = new String(bytes.toArray(ValueLayout.JAVA_BYTE), StandardCharsets.UTF_8);
      } finally {
        mlt_ffi_h.diplomat_buffer_write_destroy(write);
      }
    } catch (Throwable t) {
      msg = "(failed to read error: " + t.getMessage() + ")";
    } finally {
      mlt_ffi_h.MltError_destroy(errPtr);
    }
    throw new MltEncodeException(phase, layerName, colName, msg);
  }
}
