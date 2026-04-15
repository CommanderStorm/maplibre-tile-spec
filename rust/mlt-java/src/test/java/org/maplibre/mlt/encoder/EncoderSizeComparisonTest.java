package org.maplibre.mlt.encoder;

import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Stream;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Tag;
import org.junit.jupiter.api.Test;
import org.locationtech.jts.geom.Geometry;
import org.locationtech.jts.geom.GeometryCollection;
import org.locationtech.jts.geom.MultiLineString;
import org.locationtech.jts.geom.MultiPoint;
import org.locationtech.jts.geom.MultiPolygon;
import org.maplibre.mlt.converter.ConversionConfig;
import org.maplibre.mlt.converter.MltConverter;
import org.maplibre.mlt.converter.mvt.ColumnMappingConfig;
import org.maplibre.mlt.converter.mvt.MapboxVectorTile;
import org.maplibre.mlt.converter.mvt.MvtUtils;
import org.maplibre.mlt.metadata.tileset.MltMetadata;

/**
 * Encodes all OMT fixture tiles with both the Java and Rust/JNI encoders and prints a size
 * comparison table. Tagged as benchmark so it only runs via {@code ./gradlew benchmark}.
 */
@Tag("benchmark")
class EncoderSizeComparisonTest {

  private static final Path FIXTURES = Path.of("../../test/fixtures/omt");

  private static final ConversionConfig JAVA_CONFIG =
      ConversionConfig.builder()
          .useFastPFOR(false)
          .useFSST(false)
          .useMortonEncoding(true)
          .preTessellatePolygons(false)
          .mismatchPolicy(ConversionConfig.TypeMismatchPolicy.COERCE)
          .build();

  private static final EncoderConfig RUST_CONFIG =
      EncoderConfig.builder()
          .allowFastPfor(true)
          .allowFsst(true)
          .allowSharedDict(true)
          .trySpatialMortonSort(true)
          .trySpatialHilbertSort(true)
          .tryIdSort(true)
          .tessellate(true)
          .build();

  @BeforeAll
  static void ensureNativeLibrary() {
    assertTrue(MltEncoder.isAvailable(), "Native library must be loaded");
  }

  @Test
  void compareEncoderOutputSizes() throws IOException {
    List<Path> mvtFiles;
    try (Stream<Path> stream = Files.list(FIXTURES)) {
      mvtFiles = stream.filter(p -> p.toString().endsWith(".mvt")).sorted().toList();
    }
    assertTrue(!mvtFiles.isEmpty(), "Expected MVT fixture files in " + FIXTURES);

    String header =
        String.format("%-30s %10s %10s %10s %8s", "Tile", "MVT", "Java MLT", "Rust MLT", "Diff %");
    String separator = "-".repeat(header.length());

    System.out.println();
    System.out.println(header);
    System.out.println(separator);

    long totalMvt = 0;
    long totalJava = 0;
    long totalRust = 0;
    int javaSmaller = 0;
    int rustSmaller = 0;
    int same = 0;

    for (Path mvtPath : mvtFiles) {
      byte[] mvtBytes = Files.readAllBytes(mvtPath);
      MapboxVectorTile mvt = filterMvt(MvtUtils.decodeMvt(mvtBytes));

      // Java encoder
      MltMetadata.TileSetMetadata metadata =
          MltConverter.createTilesetMetadata(mvt, (ColumnMappingConfig) null, true);
      byte[] javaMlt = MltConverter.convertMvt(mvt, metadata, JAVA_CONFIG, null);

      // Rust/JNI encoder
      List<Layer> jniLayers = BenchmarkUtils.convertMvtToLayers(mvt);
      byte[] rustMlt = MltEncoder.encode(jniLayers, RUST_CONFIG);

      int mvtSize = mvtBytes.length;
      int javaSize = javaMlt.length;
      int rustSize = rustMlt.length;

      totalMvt += mvtSize;
      totalJava += javaSize;
      totalRust += rustSize;

      double diffPct = javaSize == 0 ? 0 : ((double) (rustSize - javaSize) / javaSize) * 100.0;

      if (rustSize < javaSize) {
        rustSmaller++;
      } else if (javaSize < rustSize) {
        javaSmaller++;
      } else {
        same++;
      }

      String name = java.util.Objects.requireNonNull(mvtPath.getFileName()).toString();
      System.out.printf(
          "%-30s %10d %10d %10d %+7.1f%%%n", name, mvtSize, javaSize, rustSize, diffPct);
    }

    System.out.println(separator);
    System.out.printf(
        "%-30s %10d %10d %10d %+7.1f%%%n",
        "TOTAL",
        totalMvt,
        totalJava,
        totalRust,
        totalJava == 0 ? 0 : ((double) (totalRust - totalJava) / totalJava) * 100.0);
    System.out.println();
    System.out.printf("Java vs MVT compression ratio:  %.2f%%%n", (totalJava * 100.0) / totalMvt);
    System.out.printf("Rust vs MVT compression ratio:  %.2f%%%n", (totalRust * 100.0) / totalMvt);
    System.out.printf(
        "Java smaller: %d | Rust smaller: %d | Same: %d (of %d tiles)%n",
        javaSmaller, rustSmaller, same, mvtFiles.size());
  }

  /**
   * Remove features with null/empty geometry or unsupported GeometryCollection types so both
   * encoders operate on identical input.
   */
  private static MapboxVectorTile filterMvt(MapboxVectorTile mvt) {
    List<org.maplibre.mlt.data.Layer> filtered = new ArrayList<>();
    for (var layer : mvt.layers()) {
      var kept =
          layer.features().stream()
              .filter(
                  f -> {
                    Geometry g = f.geometry();
                    if (g == null || g.isEmpty()) {
                      return false;
                    }
                    return !(g instanceof GeometryCollection)
                        || g instanceof MultiPoint
                        || g instanceof MultiLineString
                        || g instanceof MultiPolygon;
                  })
              .toList();
      filtered.add(new org.maplibre.mlt.data.Layer(layer.name(), kept, layer.tileExtent()));
    }
    return new MapboxVectorTile(filtered);
  }
}
