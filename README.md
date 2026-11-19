# cosmic-bundle-validator

Checks a COSMIC theme bundle with the same importer COSMIC Settings uses.

```
cosmic-bundle-validator validate <bundle.zip>
cosmic-bundle-validator build <id.xml> [--out <dir>]
```

`validate` imports the zip into a throwaway home, exactly as `cosmic-settings` would, and fails with Settings' own error if the import would.

`build` is for `cosmic-constellations`. It reads a pin file, checks it, fetches the pinned commit, zips the bundle with `git archive`, then validates it and writes
`<dir>/ID.zip`. A pin file looks like this:

```xml
<bundle id="com.example.theme">
  <source type="git" url="https://github.com/you/your-theme" commit="<full 40-char sha>"/>
  <developer id="io.github.you">
    <name>Your Name</name>
  </developer>
</bundle>
```
