# `path-loader-router`

Routes a component loader based on the path.

See [`path-loader`](../path-loader/) for the router with the loader components installed.

Based on the path prefix the request is routed to an appropriate loader:

- `http://` -> http-loader: scheme is preserved
- `https://` -> http-loader: scheme is preserved
- `file:///` -> filesystem-loader: scheme is dropped, last '/' is preserved with path 
- `file://` -> invalid, other hosts are not supported
- `file:` -> filesystem-loader: scheme is dropped
- `oci:` -> oci-loader: scheme is dropped
