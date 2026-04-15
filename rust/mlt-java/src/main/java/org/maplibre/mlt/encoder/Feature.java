package org.maplibre.mlt.encoder;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;
import java.util.OptionalLong;
import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Nullable;
import org.locationtech.jts.geom.Geometry;

/**
 * A map feature with a JTS geometry and named properties.
 *
 * @param id feature identifier (empty if absent)
 * @param geometry JTS geometry (must not be empty)
 * @param properties unmodifiable map; values: Boolean|Integer|Long|Float|Double|String|null
 */
public record Feature(
    @NotNull OptionalLong id,
    @NotNull Geometry geometry,
    @NotNull Map<String, @Nullable Object> properties) {

  public Feature {
    Objects.requireNonNull(id, "id");
    Objects.requireNonNull(geometry, "geometry");
    if (geometry.isEmpty()) {
      throw new IllegalArgumentException("Empty geometries not supported");
    }
    Objects.requireNonNull(properties, "properties");
    properties = Collections.unmodifiableMap(new LinkedHashMap<>(properties));
  }

  boolean hasId() {
    return id.isPresent();
  }
}
