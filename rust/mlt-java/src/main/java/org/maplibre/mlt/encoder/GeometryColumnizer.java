package org.maplibre.mlt.encoder;

import com.carrotsearch.hppc.IntArrayList;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.ByteOrder;
import java.util.List;
import org.locationtech.jts.geom.Coordinate;
import org.locationtech.jts.geom.Geometry;
import org.locationtech.jts.geom.LineString;
import org.locationtech.jts.geom.MultiLineString;
import org.locationtech.jts.geom.MultiPoint;
import org.locationtech.jts.geom.MultiPolygon;
import org.locationtech.jts.geom.Point;
import org.locationtech.jts.geom.Polygon;
import org.maplibre.mlt.encoder.ffi.DiplomatI32View;
import org.maplibre.mlt.encoder.ffi.DiplomatU32View;
import org.maplibre.mlt.encoder.ffi.mlt_ffi_h;

/// Builds flat coords + meta arrays from JTS geometries and passes them to
/// Rust via a single `set_geometries` FFI call.
///
/// The meta array encodes per-feature geometry type and structure metadata.
/// Rust parses this to reconstruct geo_types objects and call push_geom()
/// for each feature, handling all columnar offset bookkeeping internally.
final class GeometryColumnizer {

  private static final ValueLayout.OfInt NATIVE_INT =
      ValueLayout.JAVA_INT_UNALIGNED.withOrder(ByteOrder.nativeOrder());

  // Geometry type tags — must stay in sync with builder.rs META_* constants.
  private static final int META_POINT = 0;
  private static final int META_LINESTRING = 1;
  private static final int META_POLYGON = 2;
  private static final int META_MULTIPOINT = 3;
  private static final int META_MULTILINESTRING = 4;
  private static final int META_MULTIPOLYGON = 5;

  private GeometryColumnizer() {}

  static void write(
      MemorySegment layerPtr, Layer layer, List<Feature> features, int featureCount, Arena arena) {
    IntArrayList coords = new IntArrayList(featureCount * 4);
    IntArrayList meta = new IntArrayList(featureCount * 2);

    for (int i = 0; i < featureCount; i++) {
      Geometry geom = features.get(i).geometry();
      switch (geom) {
        case Point p -> {
          meta.add(META_POINT);
          coords.add(Math.toIntExact((long) p.getX()));
          coords.add(Math.toIntExact((long) p.getY()));
        }
        case LineString ls -> {
          meta.add(META_LINESTRING);
          meta.add(ls.getNumPoints());
          pushCoords(ls.getCoordinates(), coords);
        }
        case Polygon poly -> {
          meta.add(META_POLYGON);
          pushPolygonMeta(poly, meta);
          pushPolygonCoords(poly, coords);
        }
        case MultiPoint mp -> {
          int n = mp.getNumGeometries();
          meta.add(META_MULTIPOINT);
          meta.add(n);
          for (int j = 0; j < n; j++) {
            Point pt = (Point) mp.getGeometryN(j);
            coords.add(Math.toIntExact((long) pt.getX()));
            coords.add(Math.toIntExact((long) pt.getY()));
          }
        }
        case MultiLineString mls -> {
          int n = mls.getNumGeometries();
          var lines = new LineString[n];
          for (int j = 0; j < n; j++) {
            lines[j] = (LineString) mls.getGeometryN(j);
          }
          meta.add(META_MULTILINESTRING);
          meta.add(n);
          for (int j = 0; j < n; j++) {
            meta.add(lines[j].getNumPoints());
          }
          for (int j = 0; j < n; j++) {
            pushCoords(lines[j].getCoordinates(), coords);
          }
        }
        case MultiPolygon mp -> {
          int n = mp.getNumGeometries();
          var polys = new Polygon[n];
          for (int j = 0; j < n; j++) {
            polys[j] = (Polygon) mp.getGeometryN(j);
          }
          meta.add(META_MULTIPOLYGON);
          meta.add(n);
          for (int j = 0; j < n; j++) {
            pushPolygonMeta(polys[j], meta);
          }
          for (int j = 0; j < n; j++) {
            pushPolygonCoords(polys[j], coords);
          }
        }
        default ->
            throw new IllegalArgumentException(
                "Unsupported geometry type: " + geom.getGeometryType());
      }
    }

    MemorySegment coordsSeg = copyIntArrayList(coords, arena);
    MemorySegment coordsView =
        FfiHelpers.makeView(coordsSeg, coords.size(), DiplomatI32View.layout(), arena);

    MemorySegment metaSeg = copyIntArrayList(meta, arena);
    MemorySegment metaView =
        FfiHelpers.makeView(metaSeg, meta.size(), DiplomatU32View.layout(), arena);

    MemorySegment result =
        mlt_ffi_h.MltLayerEncoder_set_geometries(arena, layerPtr, coordsView, metaView);
    FfiHelpers.checkResult(result, MltEncodeException.Phase.SET_GEOMETRY, layer.name(), null);
  }

  /// Push all coordinates from a coordinate array.
  private static void pushCoords(Coordinate[] coordinates, IntArrayList coords) {
    for (Coordinate c : coordinates) {
      coords.add(Math.toIntExact((long) c.x));
      coords.add(Math.toIntExact((long) c.y));
    }
  }

  /// Append polygon meta: num_rings, then each ring's coordinate count.
  private static void pushPolygonMeta(Polygon poly, IntArrayList meta) {
    int numRings = 1 + poly.getNumInteriorRing();
    meta.add(numRings);
    meta.add(poly.getExteriorRing().getNumPoints());
    for (int r = 0; r < poly.getNumInteriorRing(); r++) {
      meta.add(poly.getInteriorRingN(r).getNumPoints());
    }
  }

  /// Push all ring coordinates for a polygon.
  private static void pushPolygonCoords(Polygon poly, IntArrayList coords) {
    pushCoords(poly.getExteriorRing().getCoordinates(), coords);
    for (int r = 0; r < poly.getNumInteriorRing(); r++) {
      pushCoords(poly.getInteriorRingN(r).getCoordinates(), coords);
    }
  }

  /// Bulk-copy an IntArrayList to off-heap native-endian int array.
  private static MemorySegment copyIntArrayList(IntArrayList list, Arena arena) {
    MemorySegment seg = arena.allocate((long) list.size() * 4);
    MemorySegment.copy(list.buffer, 0, seg, NATIVE_INT, 0, list.size());
    return seg;
  }
}
