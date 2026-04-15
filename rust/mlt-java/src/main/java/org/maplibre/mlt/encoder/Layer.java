package org.maplibre.mlt.encoder;

import com.google.errorprone.annotations.CanIgnoreReturnValue;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.OptionalLong;
import lombok.Builder;
import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Nullable;
import org.locationtech.jts.geom.Geometry;

/// A tile layer containing features with shared property column names.
public record Layer(
    @NotNull String name,
    int extent,
    @NotNull List<@NotNull String> propertyNames,
    @NotNull List<@NotNull Feature> features) {
  public Layer {
    Objects.requireNonNull(name, "name");
    if (extent <= 0) {
      throw new IllegalArgumentException("extent must be positive");
    }
    Objects.requireNonNull(propertyNames, "propertyNames");
    Objects.requireNonNull(features, "features");
    propertyNames = List.copyOf(propertyNames);
    features = List.copyOf(features);
  }

  @Builder(builderClassName = "LayerBuilder", builderMethodName = "builder")
  private static Layer create(
      String name, int extent, List<String> propertyNames, List<Feature> features) {
    return new Layer(name, extent, propertyNames, features);
  }

  /** Builds {@link Layer} instances from JTS geometries and plain Java property values. */
  public static class LayerBuilder {
    private @NotNull List<@NotNull String> propertyNames = List.of();
    private @NotNull List<@NotNull Feature> features = new ArrayList<>();

    /** Must be called before {@link #addFeature}. */
    @CanIgnoreReturnValue
    public @NotNull LayerBuilder propertyNames(String @NotNull ... names) {
      if (!features.isEmpty()) {
        throw new IllegalStateException("propertyNames must be set before adding features");
      }
      Objects.requireNonNull(names, "names");
      this.propertyNames = List.of(names);
      return this;
    }

    @CanIgnoreReturnValue
    public @NotNull LayerBuilder addFeature(
        long id, @NotNull Geometry geom, @Nullable Object... propertyValues) {
      features.add(buildFeature(OptionalLong.of(id), geom, propertyValues));
      return this;
    }

    @CanIgnoreReturnValue
    public @NotNull LayerBuilder addFeature(
        @NotNull Geometry geom, @Nullable Object... propertyValues) {
      features.add(buildFeature(OptionalLong.empty(), geom, propertyValues));
      return this;
    }

    private @NotNull Feature buildFeature(
        @NotNull OptionalLong id, @NotNull Geometry geom, Object @NotNull [] propertyValues) {
      Objects.requireNonNull(geom, "geometry");
      if (geom.isEmpty()) {
        throw new IllegalArgumentException("Empty geometries are not supported");
      }
      if (propertyValues.length != propertyNames.size()) {
        throw new IllegalArgumentException(
            "Expected " + propertyNames.size() + " properties but got " + propertyValues.length);
      }
      Map<String, Object> props = new LinkedHashMap<>();
      for (int i = 0; i < propertyNames.size(); i++) {
        props.put(propertyNames.get(i), validatePropertyValue(propertyValues[i]));
      }
      return new Feature(id, geom, props);
    }

    private static @Nullable Object validatePropertyValue(@Nullable Object value) {
      if (value == null) {
        return null;
      }
      return switch (value) {
        case Boolean b -> b;
        case Byte b -> (int) b;
        case Short s -> (int) s;
        case Integer n -> n;
        case Long n -> n;
        case Float f -> f;
        case Double d -> d;
        case String s -> s;
        default ->
            throw new IllegalArgumentException(
                "Unsupported property type: " + value.getClass().getName());
      };
    }
  }
}
