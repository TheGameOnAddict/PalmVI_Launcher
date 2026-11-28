// Application-local Direct3D 12 runtime selection for x64 Windows 10 builds.
// Export aliases keep these names distinct from unexported upstream variables.
const unsigned int PalmVISDKVersion = 620;
const char *const PalmVISDKPath = ".\\D3D12\\";
#pragma comment(linker, "/export:D3D12SDKVersion=PalmVISDKVersion,DATA")
#pragma comment(linker, "/export:D3D12SDKPath=PalmVISDKPath,DATA")
