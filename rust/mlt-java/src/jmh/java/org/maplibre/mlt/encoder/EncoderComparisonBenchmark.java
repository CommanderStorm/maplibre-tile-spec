package org.maplibre.mlt.encoder;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.TimeUnit;
import org.maplibre.mlt.converter.ConversionConfig;
import org.maplibre.mlt.converter.MltConverter;
import org.maplibre.mlt.converter.mvt.ColumnMappingConfig;
import org.maplibre.mlt.converter.mvt.MapboxVectorTile;
import org.maplibre.mlt.converter.mvt.MvtUtils;
import org.maplibre.mlt.metadata.tileset.MltMetadata;
import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.BenchmarkMode;
import org.openjdk.jmh.annotations.Fork;
import org.openjdk.jmh.annotations.Measurement;
import org.openjdk.jmh.annotations.Mode;
import org.openjdk.jmh.annotations.OutputTimeUnit;
import org.openjdk.jmh.annotations.Param;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.annotations.Threads;
import org.openjdk.jmh.annotations.Warmup;

/// JMH benchmark comparing the pure Java MLT encoder (mlt-core) against the JNI-Rust MLT encoder.
///
/// Run with: `./gradlew jmh`
@State(Scope.Benchmark)
@OutputTimeUnit(TimeUnit.MILLISECONDS)
@BenchmarkMode(Mode.AverageTime)
@Threads(1)
@Warmup(iterations = 5)
@Measurement(iterations = 5)
@Fork(1)
public class EncoderComparisonBenchmark {

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
          .allowSharedDict(false)
          .trySpatialMortonSort(true)
          .trySpatialHilbertSort(false)
          .tryIdSort(false)
          .tessellate(false)
          .build();

  @Param({"0_0_0.mvt", "10_532_682.mvt", "14_8298_10748.mvt"})
  String tileFile = "";

  private MapboxVectorTile mvt;
  private MltMetadata.TileSetMetadata metadata;
  private List<Layer> jniLayers;

  @Setup
  public void setup() throws IOException {
    byte[] bytes = Files.readAllBytes(FIXTURES.resolve(tileFile));
    mvt = MvtUtils.decodeMvt(bytes);
    metadata = MltConverter.createTilesetMetadata(mvt, (ColumnMappingConfig) null, true);
    jniLayers = BenchmarkUtils.convertMvtToLayers(mvt);
  }

  @Benchmark
  public byte[] javaEncoder() throws IOException {
    return MltConverter.convertMvt(mvt, metadata, JAVA_CONFIG, null);
  }

  @Benchmark
  public byte[] rustEncoder() {
    return MltEncoder.encode(jniLayers, RUST_CONFIG);
  }
}
