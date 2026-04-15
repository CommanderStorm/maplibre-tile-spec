package org.maplibre.mlt.encoder;

import java.util.ArrayList;
import java.util.List;
import org.locationtech.jts.geom.Geometry;
import org.locationtech.jts.geom.GeometryCollection;
import org.locationtech.jts.geom.MultiLineString;
import org.locationtech.jts.geom.MultiPoint;
import org.locationtech.jts.geom.MultiPolygon;
import org.maplibre.mlt.converter.mvt.MapboxVectorTile;

/** Shared utilities for encoding benchmarks. */
final class BenchmarkUtils {

  /** Convert a decoded MVT tile into the encoder's {@link Layer} list format. */
  static List<Layer> convertMvtToLayers(MapboxVectorTile mvt) {
    List<Layer> layers = new ArrayList<>();
    int totalSkipped = 0;
    for (var mvtLayer : mvt.layers()) {
      int extent = mvtLayer.tileExtent() > 0 ? mvtLayer.tileExtent() : 4096;

      String[] propNames =
          mvtLayer.features().stream()
              .flatMap(f -> f.properties().keySet().stream())
              .distinct()
              .toArray(String[]::new);

      var builder = Layer.builder().name(mvtLayer.name()).extent(extent).propertyNames(propNames);

      for (var feature : mvtLayer.features()) {
        Geometry geom = feature.geometry();
        if (geom == null || geom.isEmpty()) {
          totalSkipped++;
          continue;
        }
        if (geom instanceof GeometryCollection
            && !(geom instanceof MultiPoint)
            && !(geom instanceof MultiLineString)
            && !(geom instanceof MultiPolygon)) {
          totalSkipped++;
          continue;
        }

        Object[] values = new Object[propNames.length];
        for (int i = 0; i < propNames.length; i++) {
          values[i] = feature.properties().get(propNames[i]);
        }

        if (feature.hasId()) {
          builder.addFeature(feature.id(), geom, values);
        } else {
          builder.addFeature(geom, values);
        }
      }
      layers.add(builder.build());
    }
    if (totalSkipped > 0) {
      System.err.printf(
          "[convertMvtToLayers] Skipped %d features (empty/unsupported geometry)%n", totalSkipped);
    }
    return layers;
  }

  private BenchmarkUtils() {}
}
