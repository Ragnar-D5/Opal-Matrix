A matrix frontend (hopefully for all major platforms) written in Rust 

TODO:
- Login flow
- [ ] Server discovery via /.well-known/matrix/server
- [ ] Check API version via /_matrix/client/versions
- [ ] Get supported login flows via /_matrix/client/_VERSION_/login
  - [ ] If SSO is supported, get the issuer from /_matrix/client/v1/auth_metadata
  - [ ] Redirect to SSO with //_matrix/client/v3/login/sso/redirect?redirectUrl=_CLIENT_URL_&action=login