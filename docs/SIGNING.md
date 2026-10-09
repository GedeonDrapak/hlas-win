# Code signing (Azure Artifact Signing)

Unsigned, `Hlas-setup.exe` triggers the blue Microsoft Defender SmartScreen
warning ("Windows protected your PC"). Users can still click **More info >
Run anyway**, but most will not. Signing removes the "Unknown publisher" part
at once; the warning itself fades as the signed files collect download
reputation.

## Why Azure Artifact Signing, and why through Eden Makers

- It is Microsoft's own service and the cheapest public-trust option:
  **Basic, USD 9.99 per month**, 5,000 signatures. No hardware token.
- Public-trust certificates are issued to **organizations** in the EU, but to
  **individual developers only in the US and Canada**. So the account must be
  **Eden Makers s.r.o.**, the same decision as for Apple Developer ID
  (team `CLG46PZG7A`).
- Certificates live for one day and renew automatically; the timestamp keeps
  signed files valid forever.

## One-time setup (about an hour of clicking, then a few days of validation)

1. **Azure subscription** for Eden Makers (pay-as-you-go; a sponsored or trial
   subscription does not work). Billing starts when the signing account is
   created, even before validation succeeds.
2. In the subscription, register the resource provider
   **`Microsoft.CodeSigning`**.
3. Create an **Artifact Signing account** (Basic SKU). Pick an EU region, e.g.
   West Europe. Note the **account URI** on the Overview page, for example
   `https://weu.codesigning.azure.net/`.
4. **Identity validation > New > Public > Organization.** Use the exact legal
   name and address of Eden Makers s.r.o. as in the Czech business register.
   Microsoft may ask for documents. Wait for "Completed".
5. **Certificate profile > Public Trust**, linked to that validation. Note
   its name.
6. **Let GitHub sign without secrets that expire:** create an Entra app
   registration (or user-assigned managed identity) with a **federated
   credential** for `GedeonDrapak/hlas-win`, entity type *Tag* (pattern `v*`)
   and *Branch* `master`. Give it the **Artifact Signing Certificate Profile
   Signer** role on the signing account (older docs call it "Trusted Signing
   Certificate Profile Signer").
7. In the GitHub repo, **Settings > Secrets and variables > Actions**:

   | Kind | Name | Value |
   |---|---|---|
   | Secret | `AZURE_CLIENT_ID` | app registration (client) id |
   | Secret | `AZURE_TENANT_ID` | Eden Makers tenant id |
   | Secret | `AZURE_SUBSCRIPTION_ID` | subscription id |
   | Variable | `AZURE_SIGNING_ENDPOINT` | account URI from step 3 |
   | Variable | `AZURE_SIGNING_ACCOUNT` | signing account name |
   | Variable | `AZURE_CERTIFICATE_PROFILE` | profile name from step 5 |

That is all. The workflow (`.github/workflows/windows.yml`) signs `hlas.exe`
before packaging and `Hlas-setup.exe` after, but only when
`AZURE_SIGNING_ACCOUNT` is set, so unsigned builds keep working until then.

## Check a signed build

```powershell
Get-AuthenticodeSignature .\Hlas-setup.exe | Format-List Status, SignerCertificate
```

`Status` must be `Valid` and the signer `CN=Eden Makers s.r.o.`.
