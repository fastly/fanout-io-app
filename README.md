# Fanout.io Fastly Compute App

This app implements Fanout.io realm behavior on Fastly Compute. Essentially it forwards requests through the Fanout proxy to backends, except for some special requests that it handles directly.

The behavior is as follows:

* If the host of an incoming request ends with `.fanoutcdn.com` and the path begins with `/test` or `/bayeux`, the app will handle the request itself without forwarding to a backend.
* Otherwise, the request will be forwarded through the Fanout proxy to a backend named `https_backend_{request-host}`. Here `https` refers to the scheme used by the incoming request, which for Compute is always `https`. This means even if the backend configuration is set up to use plaintext with the backend server, the **name** of the backend must still begin with `https_`.

## Security issues

Please see [SECURITY.md](SECURITY.md) for guidance on reporting security-related issues.
