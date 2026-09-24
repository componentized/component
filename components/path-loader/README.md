# `path-loader`

Loads a component from a path.

See [`path-loader-router`](../path-loader-router/) for the router without the loader components installed.

Based on the path prefix the request is routed to an appropriate loader:

- `http://` -> http-loader: scheme is preserved
- `https://` -> http-loader: scheme is preserved
- `file:///` -> filesystem-loader: scheme is dropped, last '/' is preserved with path 
- `file://` -> invalid, other hosts are not supported
- `file:` -> filesystem-loader: scheme is dropped
- `oci://` -> oci-loader: scheme is dropped
