# RustyFin changes

This is the AccessKit Windows adapter published as `accesskit_windows` 0.34.0 from revision `c88605b96d04431f9c3c792464a0f2f253480e94`.

`src/node.rs` exposes the Windows UI Automation `HeadingLevel` property so Narrator can find headings. Return to the registry dependency once a compatible release provides this property. [Upstream issue](https://github.com/AccessKit/accesskit/issues/840)
