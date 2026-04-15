/** Java Panama FFM bindings for the Rust MLT encoder. */
module org.maplibre.mlt.encoder {
  requires transitive org.locationtech.jts;
  requires com.carrotsearch.hppc;
  requires static transitive org.jetbrains.annotations;
  requires static com.google.errorprone.annotations;
  requires static lombok;

  exports org.maplibre.mlt.encoder;
}
