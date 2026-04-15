package org.maplibre.mlt.encoder;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Random;
import java.util.concurrent.TimeUnit;
import org.locationtech.jts.geom.Coordinate;
import org.locationtech.jts.geom.GeometryFactory;
import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.BenchmarkMode;
import org.openjdk.jmh.annotations.Fork;
import org.openjdk.jmh.annotations.Level;
import org.openjdk.jmh.annotations.Measurement;
import org.openjdk.jmh.annotations.Mode;
import org.openjdk.jmh.annotations.OutputTimeUnit;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.annotations.Warmup;

/// JMH benchmarks for the Rust MLT encoder via JNI.
///
/// Data is constructed once per trial in [#setup()], simulating realistic Planetiler-style
/// workloads. Only the `encode()` call is measured.
///
/// Run with: `./gradlew jmh`
@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.MICROSECONDS)
@Warmup(iterations = 5, time = 1)
@Measurement(iterations = 10, time = 1)
@Fork(1)
@State(Scope.Benchmark)
public class MltEncoderBenchmark {

  private static final GeometryFactory GF = new GeometryFactory();
  private static final EncoderConfig DEFAULT_CONFIG = EncoderConfig.defaults();
  private static final EncoderConfig MINIMAL_CONFIG =
      EncoderConfig.builder()
          .trySpatialMortonSort(false)
          .trySpatialHilbertSort(false)
          .tryIdSort(false)
          .allowFsst(false)
          .allowFastPfor(false)
          .allowSharedDict(false)
          .build();

  private List<Layer> points500;
  private List<Layer> points5000;
  private List<Layer> lines1000;
  private List<Layer> lines5000;
  private List<Layer> polygons1000;
  private List<Layer> polygons3000;
  private List<Layer> tileSmall;
  private List<Layer> tileMedium;
  private List<Layer> tileLarge;
  private List<Layer> richProperties;

  @Setup(Level.Trial)
  public void setup() {
    if (!MltEncoder.isAvailable()) {
      throw new IllegalStateException("Native library not available — check java.library.path");
    }
    points500 = List.of(buildPointLayer(500, 5));
    points5000 = List.of(buildPointLayer(5000, 5));
    lines1000 = List.of(buildLineStringLayer(1000, 20, 4));
    lines5000 = List.of(buildLineStringLayer(5000, 20, 4));
    polygons1000 = List.of(buildPolygonLayer(1000, 8, 3));
    polygons3000 = List.of(buildPolygonLayer(3000, 8, 3));
    tileSmall = buildRealisticTile(500);
    tileMedium = buildRealisticTile(2000);
    tileLarge = buildRealisticTile(5000);
    richProperties = List.of(buildManyPropertiesLayer(1000, 15));
  }

  // --- Point layer (POI-like, e.g. OMT "place" or "poi") ---

  @Benchmark
  public byte[] encodePoints500() {
    return MltEncoder.encode(points500, DEFAULT_CONFIG);
  }

  @Benchmark
  public byte[] encodePoints5000() {
    return MltEncoder.encode(points5000, DEFAULT_CONFIG);
  }

  // --- LineString layer (road network, e.g. OMT "transportation") ---

  @Benchmark
  public byte[] encodeLines1000() {
    return MltEncoder.encode(lines1000, DEFAULT_CONFIG);
  }

  @Benchmark
  public byte[] encodeLines5000() {
    return MltEncoder.encode(lines5000, DEFAULT_CONFIG);
  }

  // --- Polygon layer (buildings, e.g. OMT "building") ---

  @Benchmark
  public byte[] encodePolygons1000() {
    return MltEncoder.encode(polygons1000, DEFAULT_CONFIG);
  }

  @Benchmark
  public byte[] encodePolygons3000() {
    return MltEncoder.encode(polygons3000, DEFAULT_CONFIG);
  }

  // --- Multi-layer tile (full OMT-like tile) ---

  @Benchmark
  public byte[] encodeTileSmall() {
    return MltEncoder.encode(tileSmall, DEFAULT_CONFIG);
  }

  @Benchmark
  public byte[] encodeTileMedium() {
    return MltEncoder.encode(tileMedium, DEFAULT_CONFIG);
  }

  @Benchmark
  public byte[] encodeTileLarge() {
    return MltEncoder.encode(tileLarge, DEFAULT_CONFIG);
  }

  // --- Property-heavy layer (many columns, mixed types) ---

  @Benchmark
  public byte[] encodeRichProperties() {
    return MltEncoder.encode(richProperties, DEFAULT_CONFIG);
  }

  // --- Config comparison: minimal optimizations on same data ---

  @Benchmark
  public byte[] encodeLines1000MinimalConfig() {
    return MltEncoder.encode(lines1000, MINIMAL_CONFIG);
  }

  // =========================================================================
  // Data builders — simulate Planetiler-style tile data
  // =========================================================================

  private static Layer buildPointLayer(int numFeatures, int numProps) {
    Random rng = new Random(42);
    String[] names =
        propNames(
            numProps,
            "name",
            "class",
            "rank",
            "population",
            "capital",
            "iso_a2",
            "admin_level",
            "ele",
            "layer",
            "subclass",
            "indoor",
            "level",
            "brunnel",
            "intermittent",
            "surface");
    String[] pNames = Arrays.copyOf(names, numProps);

    var builder = Layer.builder().name("poi").extent(4096).propertyNames(pNames);
    String[] classes = {"city", "town", "village", "hamlet", "suburb", "island", "locality"};

    for (int i = 0; i < numFeatures; i++) {
      Object[] props = new Object[numProps];
      for (int p = 0; p < numProps; p++) {
        props[p] =
            switch (p % 5) {
              case 0 -> "place_" + rng.nextInt(1000);
              case 1 -> classes[rng.nextInt(classes.length)];
              case 2 -> rng.nextInt(20);
              case 3 -> rng.nextInt(10_000_000);
              case 4 -> rng.nextBoolean();
              default -> null;
            };
      }
      builder.addFeature(
          (long) i, GF.createPoint(new Coordinate(rng.nextInt(4096), rng.nextInt(4096))), props);
    }
    return builder.build();
  }

  private static Layer buildLineStringLayer(int numFeatures, int avgVertices, int numProps) {
    Random rng = new Random(42);
    String[] pNames =
        Arrays.copyOf(
            propNames(
                numProps,
                "class",
                "subclass",
                "brunnel",
                "oneway",
                "ramp",
                "service",
                "layer",
                "level",
                "indoor",
                "bicycle",
                "foot",
                "horse"),
            numProps);

    var builder = Layer.builder().name("transportation").extent(4096).propertyNames(pNames);
    String[] classes = {
      "motorway", "trunk", "primary", "secondary", "tertiary",
      "minor", "service", "track", "path", "rail"
    };

    for (int i = 0; i < numFeatures; i++) {
      int verts = Math.max(2, avgVertices + rng.nextInt(avgVertices / 2) - avgVertices / 4);
      Coordinate[] coords = new Coordinate[verts];
      int x = rng.nextInt(4096);
      int y = rng.nextInt(4096);
      for (int v = 0; v < verts; v++) {
        coords[v] = new Coordinate(x, y);
        x = Math.clamp(x + rng.nextInt(100) - 50, 0, 4095);
        y = Math.clamp(y + rng.nextInt(100) - 50, 0, 4095);
      }

      Object[] props = new Object[numProps];
      for (int p = 0; p < numProps; p++) {
        props[p] =
            switch (p) {
              case 0 -> classes[rng.nextInt(classes.length)];
              case 1 -> rng.nextBoolean() ? "link" : (Object) null;
              case 2 -> rng.nextInt(5) == 0 ? "bridge" : (Object) null;
              case 3 -> rng.nextBoolean() ? (Object) 1 : (Object) null;
              default -> rng.nextInt(3) == 0 ? (Object) rng.nextInt(10) : (Object) null;
            };
      }
      builder.addFeature((long) i, GF.createLineString(coords), props);
    }
    return builder.build();
  }

  private static Layer buildPolygonLayer(int numFeatures, int avgVertices, int numProps) {
    Random rng = new Random(42);
    String[] pNames =
        Arrays.copyOf(
            propNames(
                numProps,
                "render_height",
                "render_min_height",
                "colour",
                "hide_3d",
                "type",
                "levels",
                "min_level",
                "material"),
            numProps);

    var builder = Layer.builder().name("building").extent(4096).propertyNames(pNames);

    for (int i = 0; i < numFeatures; i++) {
      int verts = Math.max(4, avgVertices + rng.nextInt(4) - 2);
      Coordinate[] coords = buildRandomPolygonCoords(rng, verts);

      Object[] props = new Object[numProps];
      for (int p = 0; p < numProps; p++) {
        props[p] =
            switch (p) {
              case 0 -> rng.nextInt(50) + 3;
              case 1 -> 0;
              case 2 ->
                  rng.nextInt(3) == 0
                      ? "#" + Integer.toHexString(rng.nextInt(0xFFFFFF))
                      : (Object) null;
              default -> rng.nextInt(4) == 0 ? (Object) rng.nextInt(20) : (Object) null;
            };
      }
      builder.addFeature((long) i, GF.createPolygon(coords), props);
    }
    return builder.build();
  }

  private static List<Layer> buildRealisticTile(int scale) {
    List<Layer> layers = new ArrayList<>();
    layers.add(buildPointLayer(scale / 5, 5));
    layers.add(buildLineStringLayer(scale, 20, 4));
    layers.add(buildPolygonLayer(scale / 2, 8, 3));
    layers.add(buildPointLayer(scale / 10, 3));
    layers.add(buildLineStringLayer(scale / 4, 15, 2));
    layers.add(buildPolygonLayer(scale / 10, 12, 2));

    layers.set(
        0,
        new Layer(
            "poi",
            layers.get(0).extent(),
            layers.get(0).propertyNames(),
            layers.get(0).features()));
    layers.set(
        3,
        new Layer(
            "place",
            layers.get(3).extent(),
            layers.get(3).propertyNames(),
            layers.get(3).features()));
    layers.set(
        4,
        new Layer(
            "waterway",
            layers.get(4).extent(),
            layers.get(4).propertyNames(),
            layers.get(4).features()));
    layers.set(
        5,
        new Layer(
            "landuse",
            layers.get(5).extent(),
            layers.get(5).propertyNames(),
            layers.get(5).features()));
    return layers;
  }

  private static Layer buildManyPropertiesLayer(int numFeatures, int numProps) {
    Random rng = new Random(42);
    String[] pNames = new String[numProps];
    for (int i = 0; i < numProps; i++) {
      pNames[i] = "prop_" + i;
    }

    var builder = Layer.builder().name("rich").extent(4096).propertyNames(pNames);

    for (int i = 0; i < numFeatures; i++) {
      Object[] props = new Object[numProps];
      for (int p = 0; p < numProps; p++) {
        if (rng.nextInt(5) == 0) {
          props[p] = null;
        } else {
          props[p] =
              switch (p % 6) {
                case 0 -> rng.nextBoolean();
                case 1 -> rng.nextInt(10000);
                case 2 -> (long) rng.nextInt(1_000_000);
                case 3 -> rng.nextFloat() * 100;
                case 4 -> rng.nextDouble() * 1000;
                case 5 -> "val_" + rng.nextInt(500);
                default -> null;
              };
        }
      }
      builder.addFeature(
          (long) i, GF.createPoint(new Coordinate(rng.nextInt(4096), rng.nextInt(4096))), props);
    }
    return builder.build();
  }

  // =========================================================================
  // Geometry helpers
  // =========================================================================

  private static Coordinate[] buildRandomPolygonCoords(Random rng, int numVertices) {
    int cx = rng.nextInt(3800) + 100;
    int cy = rng.nextInt(3800) + 100;
    int radius = 10 + rng.nextInt(40);

    Coordinate[] coords = new Coordinate[numVertices + 1];
    for (int v = 0; v < numVertices; v++) {
      double angle = 2 * Math.PI * v / numVertices;
      int r = radius + rng.nextInt(radius / 2);
      int x = cx + (int) (r * Math.cos(angle));
      int y = cy + (int) (r * Math.sin(angle));
      coords[v] = new Coordinate(Math.clamp(x, 0, 4095), Math.clamp(y, 0, 4095));
    }
    coords[numVertices] = coords[0];
    return coords;
  }

  private static String[] propNames(int max, String... candidates) {
    return Arrays.copyOf(candidates, Math.min(max, candidates.length));
  }
}
