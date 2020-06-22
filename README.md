# waysay

![image](https://user-images.githubusercontent.com/22216761/85361418-b6b90f00-b4d0-11ea-9beb-6ffc34f26253.png)

waysay is a native wayland client for providing desktop notifications. It aims to be a drop in replacement for swaynag.

## Usage

```bash
waysay --message "Hello, world!"

# add buttons to allow the user to trigger an action
waysay --message "Do it?" \
  --button "Yes" "echo 'I did it'" \
  --button "No" "echo 'I did not do it'"
```

To use waysay as a swaynag replacement, add the following line to your sway config:

```
swaynag_command waysay
```

## waysay vs swaynag

Most users will be better off using swaynag. Use waysay if you are interested in using Rust to write native wayland clients, and/or you want to support the Rust wayland/GUI ecosystem.

#### Missing features

Several swaynag features have not yet been implemented:

* Display of detailed message
* configuration and theming

Overall waysay is quite rough around the edges at this point, although it is usable.

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
