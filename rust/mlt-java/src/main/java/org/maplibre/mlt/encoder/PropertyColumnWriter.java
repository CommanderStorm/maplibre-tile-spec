package org.maplibre.mlt.encoder;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.List;
import org.maplibre.mlt.encoder.ffi.DiplomatBoolView;
import org.maplibre.mlt.encoder.ffi.DiplomatStringView;
import org.maplibre.mlt.encoder.ffi.DiplomatU32View;
import org.maplibre.mlt.encoder.ffi.DiplomatU8View;
import org.maplibre.mlt.encoder.ffi.mlt_ffi_h;

/// Writes property columns (scalar + string) to a native layer via FFI.
final class PropertyColumnWriter {

  private static final ValueLayout.OfInt NATIVE_INT =
      ValueLayout.JAVA_INT_UNALIGNED.withOrder(ByteOrder.nativeOrder());

  @FunctionalInterface
  interface ValueSetter {
    void set(MemorySegment seg, long offset, Object value);
  }

  @SuppressWarnings("ImmutableEnumChecker")
  enum ColumnType {
    BOOL(
        mlt_ffi_h.MltColumnType_Bool(),
        1,
        (seg, off, v) -> seg.set(mlt_ffi_h.C_BOOL, off, (Boolean) v),
        "Boolean"),
    INT(
        mlt_ffi_h.MltColumnType_I32(),
        4,
        (seg, off, v) -> seg.set(NATIVE_INT, off, ((Number) v).intValue()),
        "Number"),
    LONG(
        mlt_ffi_h.MltColumnType_I64(),
        8,
        (seg, off, v) -> seg.set(mlt_ffi_h.C_LONG_LONG, off, ((Number) v).longValue()),
        "Number"),
    FLOAT(
        mlt_ffi_h.MltColumnType_F32(),
        4,
        (seg, off, v) -> seg.set(mlt_ffi_h.C_FLOAT, off, ((Number) v).floatValue()),
        "Number"),
    DOUBLE(
        mlt_ffi_h.MltColumnType_F64(),
        8,
        (seg, off, v) -> seg.set(mlt_ffi_h.C_DOUBLE, off, ((Number) v).doubleValue()),
        "Number"),
    STRING(
        -1,
        0,
        (seg, off, v) -> {
          throw new AssertionError("use writeStringColumn");
        },
        "String");

    final int tag;
    final int byteSize;
    final ValueSetter setter;
    final String expectedType;

    ColumnType(int tag, int byteSize, ValueSetter setter, String expectedType) {
      this.tag = tag;
      this.byteSize = byteSize;
      this.setter = setter;
      this.expectedType = expectedType;
    }

    static ColumnType of(Object value) {
      return switch (value) {
        case Boolean _ -> BOOL;
        case Integer _ -> INT;
        case Long _ -> LONG;
        case Float _ -> FLOAT;
        case Double _ -> DOUBLE;
        case String _ -> STRING;
        case Number _ -> DOUBLE; // safety net for any remaining Number subclass
        default ->
            throw new IllegalArgumentException(
                "Unsupported property type: " + value.getClass().getName());
      };
    }
  }

  private PropertyColumnWriter() {}

  static void writeColumns(
      MemorySegment layerPtr, Layer layer, List<Feature> features, int featureCount, Arena arena) {
    ColumnType[] columnTypes = inferColumnTypes(layer);
    List<String> propertyNames = layer.propertyNames();

    for (int col = 0; col < propertyNames.size(); col++) {
      String colName = propertyNames.get(col);
      ColumnType type = columnTypes[col];
      MemorySegment nameView = FfiHelpers.makeStringView(colName, arena);

      if (type == ColumnType.STRING) {
        writeStringColumn(layerPtr, layer.name(), colName, nameView, features, featureCount, arena);
      } else {
        writeScalarColumn(
            layerPtr, layer.name(), colName, nameView, type, features, featureCount, arena);
      }
    }
  }

  private static void writeScalarColumn(
      MemorySegment layerPtr,
      String layerName,
      String colName,
      MemorySegment nameView,
      ColumnType type,
      List<Feature> features,
      int featureCount,
      Arena arena) {
    MemorySegment values = arena.allocate((long) featureCount * type.byteSize);
    MemorySegment present = arena.allocate(featureCount);
    for (int i = 0; i < featureCount; i++) {
      Object v = features.get(i).properties().get(colName);
      if (v != null) {
        try {
          type.setter.set(values, (long) i * type.byteSize, v);
        } catch (ClassCastException e) {
          throw new IllegalArgumentException(
              "Column '"
                  + colName
                  + "': feature "
                  + i
                  + " has type "
                  + v.getClass().getSimpleName()
                  + ", expected "
                  + type.expectedType);
        }
        present.set(mlt_ffi_h.C_BOOL, i, true);
      } else {
        present.set(mlt_ffi_h.C_BOOL, i, false);
      }
    }

    // Single unified FFI call for all scalar types
    MemorySegment dataView =
        FfiHelpers.makeView(
            values, (long) featureCount * type.byteSize, DiplomatU8View.layout(), arena);
    MemorySegment presentView =
        FfiHelpers.makeView(present, featureCount, DiplomatBoolView.layout(), arena);
    MemorySegment result =
        mlt_ffi_h.MltLayerEncoder_add_column(
            arena, layerPtr, nameView, type.tag, dataView, featureCount, presentView);
    FfiHelpers.checkResult(result, MltEncodeException.Phase.ADD_COLUMN, layerName, colName);
  }

  private static void writeStringColumn(
      MemorySegment layerPtr,
      String layerName,
      String colName,
      MemorySegment nameView,
      List<Feature> features,
      int featureCount,
      Arena arena) {
    // Bulk-push all strings in a single FFI call: concatenate UTF-8 bytes
    // into one buffer with an offset array, plus a presence bitmap.
    byte[][] encoded = new byte[featureCount][];
    MemorySegment present = arena.allocate(featureCount);
    int totalBytes = 0;
    for (int i = 0; i < featureCount; i++) {
      Object v = features.get(i).properties().get(colName);
      if (v != null) {
        encoded[i] = v.toString().getBytes(StandardCharsets.UTF_8);
        totalBytes += encoded[i].length;
        present.set(mlt_ffi_h.C_BOOL, i, true);
      } else {
        encoded[i] = null;
        present.set(mlt_ffi_h.C_BOOL, i, false);
      }
    }

    MemorySegment dataSeg = arena.allocate(Math.max(totalBytes, 1));
    MemorySegment offsetsSeg = arena.allocate((long) (featureCount + 1) * 4);
    int offset = 0;
    for (int i = 0; i < featureCount; i++) {
      offsetsSeg.set(NATIVE_INT, (long) i * 4, offset);
      if (encoded[i] != null) {
        MemorySegment.copy(
            encoded[i], 0, dataSeg, ValueLayout.JAVA_BYTE, offset, encoded[i].length);
        offset += encoded[i].length;
      }
    }
    offsetsSeg.set(NATIVE_INT, (long) featureCount * 4, offset);

    MemorySegment dataView =
        FfiHelpers.makeView(dataSeg, totalBytes, DiplomatStringView.layout(), arena);
    MemorySegment offsetsView =
        FfiHelpers.makeView(offsetsSeg, featureCount + 1, DiplomatU32View.layout(), arena);
    MemorySegment presentView =
        FfiHelpers.makeView(present, featureCount, DiplomatBoolView.layout(), arena);
    MemorySegment result =
        mlt_ffi_h.MltLayerEncoder_add_string_column(
            arena, layerPtr, nameView, dataView, offsetsView, presentView);
    FfiHelpers.checkResult(result, MltEncodeException.Phase.ADD_COLUMN, layerName, colName);
  }

  /// Scan features to find the first non-null value per column, inferring the column type.
  /// Columns that are all-null default to STRING.
  private static ColumnType[] inferColumnTypes(Layer layer) {
    int numCols = layer.propertyNames().size();
    ColumnType[] types = new ColumnType[numCols];
    for (Feature f : layer.features()) {
      boolean allResolved = true;
      for (int i = 0; i < numCols; i++) {
        if (types[i] != null) continue;
        Object val = f.properties().get(layer.propertyNames().get(i));
        if (val != null) {
          types[i] = ColumnType.of(val);
        } else {
          allResolved = false;
        }
      }
      if (allResolved) break;
    }
    for (int i = 0; i < numCols; i++) {
      if (types[i] == null) types[i] = ColumnType.STRING;
    }
    return types;
  }
}
