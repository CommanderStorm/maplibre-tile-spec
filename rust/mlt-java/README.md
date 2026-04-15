# mlt-encoder-native

Java Panama FFM bindings for the Rust [MapLibre Tile (MLT)](https://github.com/maplibre/maplibre-tile-spec) encoder.

Published to Maven Central as `org.maplibre:mlt-encoder-native`.

## Requirements

- **Java 25+** (uses [Panama Foreign Function & Memory API](https://openjdk.org/jeps/454))
- **JVM flag**: `--enable-native-access=org.maplibre.mlt.encoder` on the module path
  For **Gradle**, this is done this way:
  ```gradle
  tasks.withType(JavaExec).configureEach {
      jvmArgs '--enable-native-access=org.maplibre.mlt.encoder'
  }
  ```
  For **Maven Exec Plugin**:
  ```xml
  <configuration>
      <arguments>
          <argument>--enable-native-access=org.maplibre.mlt.encoder</argument>
      </arguments>
  </configuration>
  ```


## Installation

We provide both FAT (but portable) and architecture specific bundles.

### All-platforms JAR

A single dependency that bundles native libraries for every supported platform (but is larger):

**Gradle:**
```gradle
implementation 'org.maplibre:mlt-encoder-native:<version>'
runtimeOnly 'org.maplibre:mlt-encoder-native:<version>:all'
```

**Maven:**
```xml
<dependency>
    <groupId>org.maplibre</groupId>
    <artifactId>mlt-encoder-native</artifactId>
    <version>${mlt.version}</version>
</dependency>
<dependency>
    <groupId>org.maplibre</groupId>
    <artifactId>mlt-encoder-native</artifactId>
    <version>${mlt.version}</version>
    <classifier>all</classifier>
    <scope>runtime</scope>
</dependency>
```

### Option B: Platform-specific JAR (slim builds)

Pick the classifier matching your deployment target:

| Classifier | Platform |
|---|---|
| `linux-x86_64` | Linux x86-64 (glibc) |
| `linux-x86_64-musl` | Linux x86-64 (musl/Alpine) |
| `linux-aarch64` | Linux ARM64 (glibc) |
| `macos-aarch64` | macOS Apple Silicon |
| `windows-x86_64` | Windows x86-64 |

**Gradle:**
```gradle
implementation 'org.maplibre:mlt-encoder-native:<version>'
runtimeOnly 'org.maplibre:mlt-encoder-native:<version>:linux-x86_64'
```

**Maven:**
```xml
<dependency>
    <groupId>org.maplibre</groupId>
    <artifactId>mlt-encoder-native</artifactId>
    <version>${mlt.version}</version>
</dependency>
<dependency>
    <groupId>org.maplibre</groupId>
    <artifactId>mlt-encoder-native</artifactId>
    <version>${mlt.version}</version>
    <classifier>linux-x86_64</classifier>
    <scope>runtime</scope>
</dependency>
```

## Usage

```java
import org.maplibre.mlt.encoder.*;
import org.locationtech.jts.geom.*;

GeometryFactory gf = new GeometryFactory();

Layer buildings = Layer.builder()
    .name("buildings").extent(4096)
    .propertyNames("name", "height")
    .addFeature(1L, gf.createPoint(new Coordinate(10, 20)), "Tower", 150)
    .addFeature(2L, gf.createPoint(new Coordinate(30, 40)), "Hall", 25)
    .build();

Layer roads = Layer.builder()
    .name("roads").extent(4096)
    .propertyNames("name", "kind")
    .addFeature(gf.createLineString(...), "Main St", "highway")
    .build();

byte[] mlt = MltEncoder.encode(List.of(buildings, roads));
```

### Custom encoder configuration

```java
EncoderConfig config = EncoderConfig.builder()
    .tessellate(true)
    .allowFsst(false)
    .build();

byte[] mlt = MltEncoder.encode(List.of(buildings, roads), config);
```

### Check native library availability

```java
if (MltEncoder.isAvailable()) {
    byte[] mlt = MltEncoder.encode(layers);
} else {
    // Native library not found — check classpath and JVM flags
}
```

## Supported Geometry Types

- Point / MultiPoint
- LineString / MultiLineString
- Polygon / MultiPolygon

## Supported Property Types

`Boolean`, `Integer`, `Long`, `Float`, `Double`, `String` (nullable)

## Local Development

Build the native library and run tests:

```sh
cd rust/mlt-java
./gradlew copyNativeLib test
```

This builds `mlt-ffi` via Cargo and copies the shared library to `build/natives/` for test discovery.

### Regenerate FFI bindings

After changing the Rust FFI API (`mlt-ffi/src/`):

```sh
cd rust
just sync-ffi-bindings
```

This runs `cbindgen` to regenerate the C header and `jextract` to regenerate the Java bindings.
