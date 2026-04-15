package org.maplibre.mlt.encoder;

import static org.junit.jupiter.api.Assertions.assertAll;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import java.util.OptionalLong;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Nested;
import org.junit.jupiter.api.Test;
import org.locationtech.jts.geom.Coordinate;
import org.locationtech.jts.geom.Geometry;
import org.locationtech.jts.geom.GeometryFactory;
import org.locationtech.jts.geom.LineString;
import org.locationtech.jts.geom.LinearRing;
import org.locationtech.jts.geom.MultiLineString;
import org.locationtech.jts.geom.MultiPoint;
import org.locationtech.jts.geom.MultiPolygon;
import org.locationtech.jts.geom.Point;
import org.locationtech.jts.geom.Polygon;
import org.maplibre.mlt.data.MapLibreTile;
import org.maplibre.mlt.decoder.MltDecoder;

class MltEncoderTest {

  private static final GeometryFactory GF = new GeometryFactory();

  /** Sorting and advanced codecs disabled for deterministic round-trip order. */
  private static final EncoderConfig ROUND_TRIP_CONFIG =
      EncoderConfig.builder()
          .trySpatialMortonSort(false)
          .trySpatialHilbertSort(false)
          .tryIdSort(false)
          .allowFsst(false)
          .allowFastPfor(false)
          .allowSharedDict(false)
          .tessellate(false)
          .build();

  @BeforeAll
  static void ensureNativeLibrary() {
    assertTrue(MltEncoder.isAvailable(), "Native library must be loaded");
  }

  // ── Geometry factory helpers ──────────────────────────────────────────────

  private static Coordinate coord(double x, double y) {
    return new Coordinate(x, y);
  }

  private static Point pt(double x, double y) {
    return GF.createPoint(coord(x, y));
  }

  private static LineString ls(Coordinate... coords) {
    return GF.createLineString(coords);
  }

  private static LinearRing ring(Coordinate... coords) {
    return GF.createLinearRing(coords);
  }

  private static Polygon poly(Coordinate... coords) {
    return GF.createPolygon(coords);
  }

  private static Polygon polyWithHole(LinearRing exterior, LinearRing hole) {
    return GF.createPolygon(exterior, new LinearRing[] {hole});
  }

  private static MultiPoint mpt(Coordinate... coords) {
    return GF.createMultiPointFromCoords(coords);
  }

  // ── Assertion helpers ─────────────────────────────────────────────────────

  /** Compares geometry type and all coordinates with tolerance. */
  private static void assertGeometryEquals(Geometry expected, Geometry actual) {
    assertEquals(expected.getGeometryType(), actual.getGeometryType(), "geometry type mismatch");
    Coordinate[] expectedCoords = expected.getCoordinates();
    Coordinate[] actualCoords = actual.getCoordinates();
    assertEquals(expectedCoords.length, actualCoords.length, "coordinate count mismatch");
    for (int i = 0; i < expectedCoords.length; i++) {
      int idx = i;
      assertEquals(expectedCoords[i].x, actualCoords[i].x, 0.1, () -> "x mismatch at index " + idx);
      assertEquals(expectedCoords[i].y, actualCoords[i].y, 0.1, () -> "y mismatch at index " + idx);
    }
  }

  /** Full round-trip assertion on a single decoded feature. */
  private static void assertFeature(
      org.maplibre.mlt.data.Feature decoded,
      Long expectedId,
      Geometry expectedGeometry,
      Map<String, Object> expectedProps) {
    if (expectedId != null) {
      assertTrue(decoded.hasId(), "expected feature to have ID");
      assertEquals(expectedId.longValue(), decoded.id(), "feature ID mismatch");
    } else {
      assertFalse(decoded.hasId(), "expected feature to have no ID");
    }
    assertGeometryEquals(expectedGeometry, decoded.geometry());
    for (var entry : expectedProps.entrySet()) {
      Object actual = decoded.properties().get(entry.getKey());
      Object expected = entry.getValue();
      if (expected == null) {
        assertNull(actual, "expected null for property " + entry.getKey());
      } else if (expected instanceof Number expectedNum) {
        assertInstanceOf(Number.class, actual, "property " + entry.getKey() + " should be Number");
        assertEquals(
            expectedNum.doubleValue(),
            ((Number) actual).doubleValue(),
            0.01,
            "property " + entry.getKey() + " value mismatch");
      } else {
        assertEquals(expected, actual, "property " + entry.getKey() + " value mismatch");
      }
    }
  }

  // ── Encode / decode helpers ───────────────────────────────────────────────

  private static MapLibreTile encodeAndDecode(List<Layer> layers) throws IOException {
    byte[] encoded = MltEncoder.encode(layers, ROUND_TRIP_CONFIG);
    return MltDecoder.decodeMlTile(encoded);
  }

  private static org.maplibre.mlt.data.Layer decodeSingleLayer(Layer layer) throws IOException {
    MapLibreTile tile = encodeAndDecode(List.of(layer));
    assertEquals(1, tile.layers().size(), "Expected exactly one decoded layer");
    return tile.layers().getFirst();
  }

  // =========================================================================

  @Nested
  @DisplayName("Geometry round-trips")
  class GeometryRoundTrips {

    @Test
    @DisplayName("Point: coordinates, IDs, and properties survive encoding")
    void point() throws IOException {
      Layer layer =
          Layer.builder()
              .name("points")
              .extent(4096)
              .propertyNames("x", "y")
              .addFeature(1L, pt(100, 200), 10, 20)
              .addFeature(2L, pt(300, 400), 30, 40)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals("points", decoded.name());
      assertEquals(4096, decoded.tileExtent());
      assertEquals(2, decoded.features().size());

      assertFeature(decoded.features().get(0), 1L, pt(100, 200), Map.of("x", 10, "y", 20));
      assertFeature(decoded.features().get(1), 2L, pt(300, 400), Map.of("x", 30, "y", 40));
    }

    @Test
    @DisplayName("LineString: coordinates and properties survive encoding")
    void lineString() throws IOException {
      LineString line = ls(coord(0, 0), coord(10, 10), coord(20, 0));

      Layer layer =
          Layer.builder()
              .name("lines")
              .extent(4096)
              .propertyNames("lanes")
              .addFeature(line, 2)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(1, decoded.features().size());
      var f = decoded.features().getFirst();
      assertGeometryEquals(line, f.geometry());
      assertInstanceOf(LineString.class, f.geometry());
      assertEquals(2, ((Number) f.properties().get("lanes")).intValue());
    }

    @Test
    @DisplayName("Polygon: ring coordinates and properties survive encoding")
    void polygon() throws IOException {
      Polygon expected =
          poly(coord(0, 0), coord(100, 0), coord(100, 100), coord(0, 100), coord(0, 0));

      Layer layer =
          Layer.builder()
              .name("polygons")
              .extent(4096)
              .propertyNames("area")
              .addFeature(1L, expected, 10000.0)
              .build();

      var decoded = decodeSingleLayer(layer);
      var f = decoded.features().getFirst();
      assertInstanceOf(Polygon.class, f.geometry());
      Polygon decodedPoly = (Polygon) f.geometry();
      assertEquals(5, decodedPoly.getExteriorRing().getNumPoints());
      assertEquals(0, decodedPoly.getNumInteriorRing());
      assertGeometryEquals(expected, decodedPoly);
      assertEquals(10000.0, ((Number) f.properties().get("area")).doubleValue(), 0.01);
    }

    @Test
    @DisplayName("Polygon with hole: interior ring coordinates preserved")
    void polygonWithHole() throws IOException {
      LinearRing exterior =
          ring(coord(0, 0), coord(200, 0), coord(200, 200), coord(0, 200), coord(0, 0));
      LinearRing hole =
          ring(coord(50, 50), coord(150, 50), coord(150, 150), coord(50, 150), coord(50, 50));
      Polygon expected = polyWithHole(exterior, hole);

      Layer layer =
          Layer.builder()
              .name("buildings")
              .extent(4096)
              .propertyNames("height")
              .addFeature(1L, expected, 15)
              .build();

      var decoded = decodeSingleLayer(layer);
      var geom = decoded.features().getFirst().geometry();
      assertInstanceOf(Polygon.class, geom);
      Polygon decodedPoly = (Polygon) geom;
      assertEquals(1, decodedPoly.getNumInteriorRing());
      assertEquals(5, decodedPoly.getInteriorRingN(0).getNumPoints());
      assertGeometryEquals(expected, decodedPoly);
    }

    @Test
    @DisplayName("MultiPoint: coordinates and properties survive encoding")
    void multiPoint() throws IOException {
      MultiPoint expected = mpt(coord(10, 20), coord(30, 40));

      Layer layer =
          Layer.builder()
              .name("multi")
              .extent(4096)
              .propertyNames("size")
              .addFeature(expected, 5)
              .build();

      var decoded = decodeSingleLayer(layer);
      var f = decoded.features().getFirst();
      assertInstanceOf(MultiPoint.class, f.geometry());
      assertEquals(2, f.geometry().getNumGeometries());
      assertGeometryEquals(expected, f.geometry());
      assertEquals(5, ((Number) f.properties().get("size")).intValue());
    }

    @Test
    @DisplayName("MultiLineString: coordinates and properties survive encoding")
    void multiLineString() throws IOException {
      LineString l1 = ls(coord(0, 0), coord(10, 10));
      LineString l2 = ls(coord(20, 20), coord(30, 30), coord(40, 40));
      MultiLineString expected = GF.createMultiLineString(new LineString[] {l1, l2});

      Layer layer =
          Layer.builder()
              .name("routes")
              .extent(4096)
              .propertyNames("priority")
              .addFeature(1L, expected, 3)
              .build();

      var decoded = decodeSingleLayer(layer);
      var f = decoded.features().getFirst();
      assertInstanceOf(MultiLineString.class, f.geometry());
      assertEquals(2, f.geometry().getNumGeometries());
      assertEquals(2, f.geometry().getGeometryN(0).getNumPoints());
      assertEquals(3, f.geometry().getGeometryN(1).getNumPoints());
      assertGeometryEquals(expected, f.geometry());
      assertEquals(3, ((Number) f.properties().get("priority")).intValue());
    }

    @Test
    @DisplayName("MultiPolygon: coordinates and properties survive encoding")
    void multiPolygon() throws IOException {
      Polygon p1 = poly(coord(0, 0), coord(10, 0), coord(10, 10), coord(0, 0));
      Polygon p2 = poly(coord(20, 20), coord(30, 20), coord(30, 30), coord(20, 20));
      MultiPolygon expected = GF.createMultiPolygon(new Polygon[] {p1, p2});

      Layer layer =
          Layer.builder()
              .name("regions")
              .extent(4096)
              .propertyNames("id")
              .addFeature(1L, expected, 42)
              .build();

      var decoded = decodeSingleLayer(layer);
      var f = decoded.features().getFirst();
      assertInstanceOf(MultiPolygon.class, f.geometry());
      assertEquals(2, f.geometry().getNumGeometries());
      assertGeometryEquals(expected, f.geometry());
      assertEquals(42, ((Number) f.properties().get("id")).intValue());
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("Property type round-trips")
  class PropertyTypes {

    @Test
    @DisplayName("all scalar types: bool, int, long, float, double, string")
    void allScalarTypes() throws IOException {
      Layer layer =
          Layer.builder()
              .name("typed")
              .extent(4096)
              .propertyNames("b", "i", "l", "f", "d", "s")
              .addFeature(1L, pt(10, 20), true, 42, 123456789L, 3.14f, 2.71828, "text")
              // second feature with null string makes column nullable (required by Java decoder)
              .addFeature(2L, pt(30, 40), false, 0, 0L, 0.0f, 0.0, null)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(2, decoded.features().size());
      var props = decoded.features().get(0).properties();

      assertEquals(true, props.get("b"));
      assertEquals(42, ((Number) props.get("i")).intValue());
      assertEquals(123456789L, ((Number) props.get("l")).longValue());
      assertEquals(3.14f, ((Number) props.get("f")).floatValue(), 0.01f);
      assertEquals(2.71828, ((Number) props.get("d")).doubleValue(), 0.001);
      assertEquals("text", props.get("s"));

      var nullProps = decoded.features().get(1).properties();
      assertNull(nullProps.get("s"));
    }

    @Test
    @DisplayName("null properties decode as absent or null")
    void nullableProperties() throws IOException {
      Layer layer =
          Layer.builder()
              .name("nullable")
              .extent(4096)
              .propertyNames("name", "value")
              .addFeature(1L, pt(10, 20), "hello", 42)
              .addFeature(2L, pt(30, 40), null, null)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(2, decoded.features().size());

      var f0 = decoded.features().get(0).properties();
      assertEquals("hello", f0.get("name"));
      assertEquals(42, ((Number) f0.get("value")).intValue());

      var f1 = decoded.features().get(1).properties();
      assertNull(f1.get("name"));
      assertNull(f1.get("value"));
    }

    @Test
    @DisplayName("all-null columns infer type and round-trip")
    void allNullColumns() throws IOException {
      Layer layer =
          Layer.builder()
              .name("typed_nulls")
              .extent(4096)
              .propertyNames("count", "flag")
              .addFeature(1L, pt(10, 20), null, null)
              .build();

      var decoded = decodeSingleLayer(layer);
      var props = decoded.features().getFirst().properties();
      assertNull(props.get("count"));
      assertNull(props.get("flag"));
    }

    @Test
    @DisplayName("duplicate string values round-trip correctly")
    void duplicateStringValues() throws IOException {
      var builder = Layer.builder().name("strings").extent(4096).propertyNames("name");
      for (int i = 0; i < 10; i++) {
        // All features share the same string value — exercises dict/FSST encoding
        builder.addFeature((long) i, pt(i, i), "shared_value");
      }
      // Add one null so the column is nullable
      builder.addFeature(10L, pt(10, 10), (Object) null);

      var decoded = decodeSingleLayer(builder.build());
      assertEquals(11, decoded.features().size());
      for (int i = 0; i < 10; i++) {
        assertEquals(
            "shared_value",
            decoded.features().get(i).properties().get("name"),
            "feature " + i + " should have shared_value");
      }
      assertNull(decoded.features().get(10).properties().get("name"));
    }

    @Test
    @DisplayName("multiple features with many properties")
    void manyProperties() throws IOException {
      Layer layer =
          Layer.builder()
              .name("rich")
              .extent(4096)
              .propertyNames("a", "b", "c", "d", "e")
              .addFeature(1L, pt(1, 2), 10, 20, 30, 40, 50)
              .addFeature(2L, pt(3, 4), 60, 70, 80, 90, 100)
              .addFeature(3L, pt(5, 6), null, null, null, null, null)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(3, decoded.features().size());

      assertFeature(
          decoded.features().get(0),
          1L,
          pt(1, 2),
          Map.of("a", 10, "b", 20, "c", 30, "d", 40, "e", 50));
      assertFeature(
          decoded.features().get(1),
          2L,
          pt(3, 4),
          Map.of("a", 60, "b", 70, "c", 80, "d", 90, "e", 100));
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("Feature ID handling")
  class FeatureIds {

    @Test
    @DisplayName("features without explicit IDs — properties and coordinates preserved")
    void withoutIds() throws IOException {
      Layer layer =
          Layer.builder()
              .name("noid")
              .extent(4096)
              .propertyNames("x")
              .addFeature(pt(1, 2), 100)
              .addFeature(pt(3, 4), 200)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(2, decoded.features().size());
      assertFeature(decoded.features().get(0), null, pt(1, 2), Map.of("x", 100));
      assertFeature(decoded.features().get(1), null, pt(3, 4), Map.of("x", 200));
    }

    @Test
    @DisplayName("sequential IDs round-trip correctly")
    void withSequentialIds() throws IOException {
      Layer layer =
          Layer.builder()
              .name("withid")
              .extent(4096)
              .addFeature(1L, pt(1, 2))
              .addFeature(2L, pt(3, 4))
              .addFeature(3L, pt(5, 6))
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(3, decoded.features().size());
      for (int i = 0; i < 3; i++) {
        var f = decoded.features().get(i);
        assertTrue(f.hasId(), "Feature " + i + " should have ID");
        assertEquals(i + 1L, f.id());
      }
    }

    @Test
    @DisplayName("large IDs (Long.MAX_VALUE) round-trip correctly")
    void largeIds() throws IOException {
      Layer layer =
          Layer.builder()
              .name("large")
              .extent(4096)
              .propertyNames("v")
              .addFeature(Long.MAX_VALUE, pt(10, 20), 1)
              .addFeature(Long.MAX_VALUE - 1, pt(30, 40), 2)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(2, decoded.features().size());
      assertTrue(decoded.features().get(0).hasId());
      assertEquals(Long.MAX_VALUE, decoded.features().get(0).id());
      assertTrue(decoded.features().get(1).hasId());
      assertEquals(Long.MAX_VALUE - 1, decoded.features().get(1).id());
    }

    @Test
    @DisplayName("sparse IDs round-trip correctly")
    void sparseIds() throws IOException {
      Layer layer =
          Layer.builder()
              .name("sparse")
              .extent(4096)
              .addFeature(1L, pt(1, 1))
              .addFeature(1000L, pt(2, 2))
              .addFeature(999999L, pt(3, 3))
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(3, decoded.features().size());
      assertEquals(1L, decoded.features().get(0).id());
      assertEquals(1000L, decoded.features().get(1).id());
      assertEquals(999999L, decoded.features().get(2).id());
    }

    @Test
    @DisplayName("zero ID round-trips correctly")
    void zeroId() throws IOException {
      Layer layer =
          Layer.builder()
              .name("zero")
              .extent(4096)
              .propertyNames("v")
              .addFeature(0L, pt(5, 5), 42)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(1, decoded.features().size());
      assertTrue(decoded.features().getFirst().hasId());
      assertEquals(0L, decoded.features().getFirst().id());
      assertEquals(42, ((Number) decoded.features().getFirst().properties().get("v")).intValue());
    }

    @Test
    @DisplayName("mixed ID presence: some features with IDs, some without")
    void mixedIdPresence() throws IOException {
      Layer layer =
          Layer.builder()
              .name("mixed")
              .extent(4096)
              .propertyNames("v")
              .addFeature(1L, pt(10, 20), 100)
              .addFeature(pt(30, 40), 200)
              .addFeature(3L, pt(50, 60), 300)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(3, decoded.features().size());

      // Verify property values survive regardless of ID presence
      assertEquals(100, ((Number) decoded.features().get(0).properties().get("v")).intValue());
      assertEquals(200, ((Number) decoded.features().get(1).properties().get("v")).intValue());
      assertEquals(300, ((Number) decoded.features().get(2).properties().get("v")).intValue());

      // Verify coordinates
      assertGeometryEquals(pt(10, 20), decoded.features().get(0).geometry());
      assertGeometryEquals(pt(30, 40), decoded.features().get(1).geometry());
      assertGeometryEquals(pt(50, 60), decoded.features().get(2).geometry());
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("Edge cases")
  class EdgeCases {

    @Test
    @DisplayName("empty layer encodes without error")
    void emptyLayer() {
      Layer layer = Layer.builder().name("empty").extent(4096).propertyNames("x").build();

      // Empty layers produce valid encoded output; the Java decoder does not yet
      // support the NONE encoding used for empty geometry columns, so we only
      // verify that encoding itself succeeds and produces bytes.
      byte[] encoded = MltEncoder.encode(List.of(layer), ROUND_TRIP_CONFIG);
      assertTrue(encoded.length > 0, "Empty layer should still produce header bytes");
    }

    @Test
    @DisplayName("single feature without properties")
    void singleFeatureNoProperties() throws IOException {
      Layer layer = Layer.builder().name("single").extent(4096).addFeature(1L, pt(50, 50)).build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(1, decoded.features().size());
      assertEquals(1L, decoded.features().getFirst().id());
      assertGeometryEquals(pt(50, 50), decoded.features().getFirst().geometry());
    }

    @Test
    @DisplayName("1000 features round-trip with correct count and spot-checked values")
    void manyFeatures() throws IOException {
      var builder = Layer.builder().name("many").extent(4096).propertyNames("idx");
      for (int i = 0; i < 1000; i++) {
        builder.addFeature((long) i, pt(i, i * 2), i);
      }

      var decoded = decodeSingleLayer(builder.build());
      assertEquals(1000, decoded.features().size());

      // Spot-check first, middle, and last features
      assertFeature(decoded.features().get(0), 0L, pt(0, 0), Map.of("idx", 0));
      assertFeature(decoded.features().get(499), 499L, pt(499, 998), Map.of("idx", 499));
      assertFeature(decoded.features().get(999), 999L, pt(999, 1998), Map.of("idx", 999));
    }

    @Test
    @DisplayName("negative coordinates round-trip correctly")
    void negativeCoordinates() throws IOException {
      Layer layer =
          Layer.builder()
              .name("neg")
              .extent(4096)
              .propertyNames("v")
              .addFeature(1L, pt(-50, -100), 1)
              .addFeature(2L, pt(-1, -1), 2)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(2, decoded.features().size());
      assertFeature(decoded.features().get(0), 1L, pt(-50, -100), Map.of("v", 1));
      assertFeature(decoded.features().get(1), 2L, pt(-1, -1), Map.of("v", 2));
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("Multi-layer tiles")
  class MultiLayerTiles {

    @Test
    @DisplayName("two layers round-trip with correct names and features")
    void twoLayers() throws IOException {
      Layer points =
          Layer.builder()
              .name("points")
              .extent(4096)
              .propertyNames("rank")
              .addFeature(1L, pt(10, 20), 1)
              .build();

      Layer lines =
          Layer.builder()
              .name("lines")
              .extent(4096)
              .propertyNames("lanes")
              .addFeature(1L, ls(coord(0, 0), coord(100, 100)), 2)
              .build();

      MapLibreTile decoded = encodeAndDecode(List.of(points, lines));

      assertEquals(2, decoded.layers().size());
      assertEquals("points", decoded.layers().get(0).name());
      assertEquals("lines", decoded.layers().get(1).name());
      assertEquals(1, decoded.layers().get(0).features().size());
      assertEquals(1, decoded.layers().get(1).features().size());

      assertInstanceOf(Point.class, decoded.layers().get(0).features().getFirst().geometry());
      assertInstanceOf(LineString.class, decoded.layers().get(1).features().getFirst().geometry());
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("Encoder configuration")
  class EncoderConfiguration {

    @Test
    @DisplayName("defaults match documented values")
    void defaults() {
      EncoderConfig defaults = EncoderConfig.defaults();
      assertAll(
          () -> assertFalse(defaults.tessellate()),
          () -> assertTrue(defaults.tryMortonSort()),
          () -> assertTrue(defaults.tryHilbertSort()),
          () -> assertTrue(defaults.tryIdSort()),
          () -> assertTrue(defaults.allowFsst()),
          () -> assertTrue(defaults.allowFastPfor()),
          () -> assertTrue(defaults.allowSharedDict()));
    }

    @Test
    @DisplayName("tessellation produces decodable polygon output")
    void tessellation() throws IOException {
      Polygon p = poly(coord(0, 0), coord(100, 0), coord(100, 100), coord(0, 100), coord(0, 0));

      Layer layer =
          Layer.builder().name("tess").extent(4096).propertyNames("x").addFeature(1L, p, 1).build();

      EncoderConfig config = EncoderConfig.builder().tessellate(true).build();
      byte[] encoded = MltEncoder.encode(List.of(layer), config);
      MapLibreTile decoded = MltDecoder.decodeMlTile(encoded);
      assertEquals(1, decoded.layers().getFirst().features().size());
    }

    @Test
    @DisplayName("encode without explicit config uses defaults and is decodable")
    void encodeDefaultConfig() throws IOException {
      Layer layer =
          Layer.builder()
              .name("defcfg")
              .extent(4096)
              .propertyNames("v")
              .addFeature(1L, pt(10, 20), 42)
              .build();

      byte[] encoded = MltEncoder.encode(List.of(layer));
      MapLibreTile decoded = MltDecoder.decodeMlTile(encoded);
      assertEquals(1, decoded.layers().size());
      assertEquals("defcfg", decoded.layers().getFirst().name());
      assertEquals(
          42,
          ((Number) decoded.layers().getFirst().features().getFirst().properties().get("v"))
              .intValue());
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("Input validation")
  class Validation {

    @Test
    @DisplayName("Feature rejects null geometry")
    void featureRejectsNullGeometry() {
      assertThrows(
          NullPointerException.class, () -> new Feature(OptionalLong.empty(), null, Map.of()));
    }

    @Test
    @DisplayName("Feature rejects empty geometry")
    void featureRejectsEmptyGeometry() {
      assertThrows(
          IllegalArgumentException.class,
          () -> new Feature(OptionalLong.empty(), GF.createPoint(), Map.of()));
    }

    @Test
    @DisplayName("Layer rejects null name")
    void layerRejectsNullName() {
      assertThrows(NullPointerException.class, () -> new Layer(null, 4096, List.of(), List.of()));
    }

    @Test
    @DisplayName("Layer rejects non-positive extent")
    void layerRejectsNonPositiveExtent() {
      assertThrows(IllegalArgumentException.class, () -> new Layer("x", 0, List.of(), List.of()));
    }

    @Test
    @DisplayName("LayerBuilder rejects wrong number of properties")
    void layerBuilderRejectsPropertyCountMismatch() {
      var builder = Layer.builder().name("test").extent(4096).propertyNames("a", "b");
      assertThrows(IllegalArgumentException.class, () -> builder.addFeature(pt(1, 2), "only_one"));
    }

    @Test
    @DisplayName("LayerBuilder rejects null geometry")
    void layerBuilderRejectsNullGeometry() {
      var builder = Layer.builder().name("test").extent(4096);
      assertThrows(NullPointerException.class, () -> builder.addFeature(null));
    }

    @Test
    @DisplayName("LayerBuilder rejects empty geometry")
    void layerBuilderRejectsEmptyGeometry() {
      var builder = Layer.builder().name("test").extent(4096);
      assertThrows(IllegalArgumentException.class, () -> builder.addFeature(GF.createPoint()));
    }

    @Test
    @DisplayName("LayerBuilder rejects propertyNames after features added")
    void layerBuilderRejectsPropertyNamesAfterFeatures() {
      var builder = Layer.builder().name("test").extent(4096);
      builder.addFeature(pt(1, 2));
      assertThrows(IllegalStateException.class, () -> builder.propertyNames("x"));
    }

    @Test
    @DisplayName("Coordinate exceeding int range throws ArithmeticException")
    void coordinateOverflowThrows() {
      // Coordinate with value exceeding Integer.MAX_VALUE
      Point overflowPt = GF.createPoint(new Coordinate(3_000_000_000.0, 1));
      Layer layer = Layer.builder().name("overflow").extent(4096).addFeature(overflowPt).build();
      assertThrows(
          ArithmeticException.class, () -> MltEncoder.encode(List.of(layer), ROUND_TRIP_CONFIG));
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("Widened property types")
  class WidenedPropertyTypes {

    @Test
    @DisplayName("Short property is coerced to Integer")
    void shortPropertyIsCoercedToInteger() throws IOException {
      Layer layer =
          Layer.builder()
              .name("coerce")
              .extent(4096)
              .propertyNames("val")
              .addFeature(1L, pt(10, 20), (short) 42)
              .addFeature(2L, pt(30, 40), (Object) null)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(42, ((Number) decoded.features().get(0).properties().get("val")).intValue());
    }

    @Test
    @DisplayName("Byte property is coerced to Integer")
    void bytePropertyIsCoercedToInteger() throws IOException {
      Layer layer =
          Layer.builder()
              .name("coerce")
              .extent(4096)
              .propertyNames("val")
              .addFeature(1L, pt(10, 20), (byte) 7)
              .addFeature(2L, pt(30, 40), (Object) null)
              .build();

      var decoded = decodeSingleLayer(layer);
      assertEquals(7, ((Number) decoded.features().get(0).properties().get("val")).intValue());
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("Structured exceptions")
  class StructuredExceptions {

    @Test
    @DisplayName("MltEncodeException includes phase and layer name")
    void encodeExceptionIncludesPhaseAndLayerName() {
      var ex =
          new MltEncodeException(
              MltEncodeException.Phase.ADD_COLUMN, "buildings", "height", "test error");

      assertEquals(MltEncodeException.Phase.ADD_COLUMN, ex.phase());
      assertEquals("buildings", ex.layerName());
      assertEquals("height", ex.columnName());
      assertTrue(ex.getMessage().contains("buildings"));
      assertTrue(ex.getMessage().contains("height"));
      assertTrue(ex.getMessage().contains("test error"));
    }

    @Test
    @DisplayName("MltEncodeException without layer/column context")
    void encodeExceptionWithoutContext() {
      var ex =
          new MltEncodeException(MltEncodeException.Phase.TILE_ENCODE, null, null, "encode failed");

      assertEquals(MltEncodeException.Phase.TILE_ENCODE, ex.phase());
      assertNull(ex.layerName());
      assertNull(ex.columnName());
      assertTrue(ex.getMessage().contains("encode failed"));
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("Mixed geometry types")
  class MixedGeometryTypes {

    @Test
    @DisplayName("Point, LineString, and Polygon in one layer round-trip")
    void mixedGeometryTypesRoundTrip() throws IOException {
      Point point = pt(10, 20);
      LineString line = ls(coord(0, 0), coord(50, 50), coord(100, 0));
      Polygon polygon =
          poly(coord(0, 0), coord(100, 0), coord(100, 100), coord(0, 100), coord(0, 0));

      // A feature with null tag is needed so the string column is nullable
      // (the Java decoder does not support non-nullable string columns).
      java.util.HashMap<String, Object> nullTag = new java.util.HashMap<>();
      nullTag.put("tag", null);

      Layer layer =
          new Layer(
              "mixed_geom",
              4096,
              List.of("tag"),
              List.of(
                  new Feature(OptionalLong.of(1), point, Map.of("tag", "pt")),
                  new Feature(OptionalLong.of(2), line, Map.of("tag", "ls")),
                  new Feature(OptionalLong.of(3), polygon, Map.of("tag", "poly")),
                  new Feature(OptionalLong.of(4), pt(0, 0), nullTag)));

      var decoded = decodeSingleLayer(layer);
      assertEquals(4, decoded.features().size());

      assertInstanceOf(Point.class, decoded.features().get(0).geometry());
      assertGeometryEquals(point, decoded.features().get(0).geometry());
      assertEquals("pt", decoded.features().get(0).properties().get("tag"));

      assertInstanceOf(LineString.class, decoded.features().get(1).geometry());
      assertGeometryEquals(line, decoded.features().get(1).geometry());
      assertEquals("ls", decoded.features().get(1).properties().get("tag"));

      assertInstanceOf(Polygon.class, decoded.features().get(2).geometry());
      assertGeometryEquals(polygon, decoded.features().get(2).geometry());
      assertEquals("poly", decoded.features().get(2).properties().get("tag"));

      assertNull(decoded.features().get(3).properties().get("tag"));
    }

    @Test
    @DisplayName("CLI validates mixed geometry encoded output")
    void mixedGeometryCliValidation() throws IOException, InterruptedException {
      Point point = pt(10, 20);
      LineString line = ls(coord(0, 0), coord(50, 50), coord(100, 0));
      Polygon polygon =
          poly(coord(0, 0), coord(100, 0), coord(100, 100), coord(0, 100), coord(0, 0));

      Layer layer =
          new Layer(
              "mixed_geom",
              4096,
              List.of("tag"),
              List.of(
                  new Feature(OptionalLong.of(1), point, Map.of("tag", "pt")),
                  new Feature(OptionalLong.of(2), line, Map.of("tag", "ls")),
                  new Feature(OptionalLong.of(3), polygon, Map.of("tag", "poly"))));

      byte[] encoded = MltEncoder.encode(List.of(layer), ROUND_TRIP_CONFIG);
      Path tmpFile = Files.createTempFile("mlt-mixed-geom-", ".mlt");
      try {
        Files.write(tmpFile, encoded);

        Path mltBinary =
            Path.of(System.getProperty("user.dir")).resolve("../target/release/mlt").normalize();
        assertTrue(Files.isExecutable(mltBinary), "mlt binary must exist at " + mltBinary);

        Process process =
            new ProcessBuilder(mltBinary.toString(), "decode", tmpFile.toString())
                .redirectErrorStream(true)
                .start();
        int exitCode = process.waitFor();
        assertEquals(0, exitCode, "mlt decode should exit with code 0 for valid MLT");
      } finally {
        Files.deleteIfExists(tmpFile);
      }
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("Type mismatch detection")
  class TypeMismatchDetection {

    @Test
    @DisplayName("Integer then String in same column produces clear IllegalArgumentException")
    void intThenStringProducesClearError() {
      Layer layer =
          new Layer(
              "mixed_types",
              4096,
              List.of("x"),
              List.of(
                  new Feature(OptionalLong.empty(), pt(1, 2), Map.of("x", 42)),
                  new Feature(OptionalLong.empty(), pt(3, 4), Map.of("x", "hello"))));

      var ex =
          assertThrows(
              IllegalArgumentException.class,
              () -> MltEncoder.encode(List.of(layer), ROUND_TRIP_CONFIG));

      assertTrue(
          ex.getMessage().contains("Column 'x'"),
          "Message should contain column name, was: " + ex.getMessage());
      assertTrue(
          ex.getMessage().contains("feature 1"),
          "Message should contain feature index, was: " + ex.getMessage());
    }

    @Test
    @DisplayName("Boolean then Integer in same column produces clear IllegalArgumentException")
    void boolThenIntProducesClearError() {
      Layer layer =
          new Layer(
              "bool_int",
              4096,
              List.of("flag"),
              List.of(
                  new Feature(OptionalLong.empty(), pt(1, 2), Map.of("flag", true)),
                  new Feature(OptionalLong.empty(), pt(3, 4), Map.of("flag", 42))));

      var ex =
          assertThrows(
              IllegalArgumentException.class,
              () -> MltEncoder.encode(List.of(layer), ROUND_TRIP_CONFIG));

      assertTrue(
          ex.getMessage().contains("Column 'flag'"),
          "Message should contain column name, was: " + ex.getMessage());
      assertTrue(
          ex.getMessage().contains("feature 1"),
          "Message should contain feature index, was: " + ex.getMessage());
      assertTrue(
          ex.getMessage().contains("expected Boolean"),
          "Message should contain expected type, was: " + ex.getMessage());
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("CLI validation")
  class CliValidation {

    @Test
    @DisplayName("Encoded multi-property layer passes CLI decode")
    void encodedOutputPassesCliDecode() throws IOException, InterruptedException {
      Layer layer =
          Layer.builder()
              .name("cli_test")
              .extent(4096)
              .propertyNames("name", "count", "active")
              .addFeature(1L, pt(10, 20), "alpha", 100, true)
              .addFeature(2L, pt(30, 40), "beta", 200, false)
              .addFeature(3L, pt(50, 60), null, null, null)
              .build();

      byte[] encoded = MltEncoder.encode(List.of(layer), ROUND_TRIP_CONFIG);
      Path tmpFile = Files.createTempFile("mlt-cli-", ".mlt");
      try {
        Files.write(tmpFile, encoded);

        Path mltBinary =
            Path.of(System.getProperty("user.dir")).resolve("../target/release/mlt").normalize();
        assertTrue(Files.isExecutable(mltBinary), "mlt binary must exist at " + mltBinary);

        Process process =
            new ProcessBuilder(mltBinary.toString(), "decode", tmpFile.toString())
                .redirectErrorStream(true)
                .start();
        String output = new String(process.getInputStream().readAllBytes());
        int exitCode = process.waitFor();
        assertEquals(0, exitCode, "mlt decode failed with output:\n" + output);
      } finally {
        Files.deleteIfExists(tmpFile);
      }
    }
  }

  // =========================================================================

  @Nested
  @DisplayName("Spatial sort round-trip")
  class SpatialSortRoundTrip {

    @Test
    @DisplayName("Default config (sorts enabled) preserves feature count and properties")
    void defaultConfigPreservesFeaturesAndProperties() throws IOException {
      var builder = Layer.builder().name("sorted").extent(4096).propertyNames("label", "val");
      builder.addFeature(1L, pt(100, 200), "a", 10);
      builder.addFeature(2L, pt(50, 400), "b", 20);
      builder.addFeature(3L, pt(300, 100), "c", 30);
      builder.addFeature(4L, pt(10, 10), "d", 40);
      builder.addFeature(5L, pt(500, 500), null, null);

      EncoderConfig defaults = EncoderConfig.defaults();
      byte[] encoded = MltEncoder.encode(List.of(builder.build()), defaults);
      MapLibreTile decoded = MltDecoder.decodeMlTile(encoded);

      assertEquals(1, decoded.layers().size());
      var decodedLayer = decoded.layers().getFirst();
      assertEquals(5, decodedLayer.features().size());

      // Order may differ due to spatial sort, so collect decoded values and assert as sets
      var decodedLabels = new java.util.HashSet<String>();
      var decodedVals = new java.util.HashSet<Integer>();
      for (var f : decodedLayer.features()) {
        Object label = f.properties().get("label");
        if (label != null) {
          decodedLabels.add(label.toString());
        }
        Object val = f.properties().get("val");
        if (val != null) {
          decodedVals.add(((Number) val).intValue());
        }
      }
      assertEquals(java.util.Set.of("a", "b", "c", "d"), decodedLabels);
      assertEquals(java.util.Set.of(10, 20, 30, 40), decodedVals);
    }
  }
}
