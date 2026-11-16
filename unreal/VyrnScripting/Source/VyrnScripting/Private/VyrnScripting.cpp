#include "VyrnScripting.h"

#include "HAL/PlatformProcess.h"
#include "Interfaces/IPluginManager.h"
#include "Misc/MessageDialog.h"
#include "Misc/Paths.h"
#include "vyrn.h"

DEFINE_LOG_CATEGORY(LogVyrn);

static bool GVyrnLoaded = false;

void FVyrnScriptingModule::StartupModule()
{
#if PLATFORM_WINDOWS || PLATFORM_MAC
	// vyrn.dll is delay-loaded: load it now from the plugin's binaries (in
	// the editor) or next to the executable (in a packaged game).
	const FString Name = PLATFORM_WINDOWS ? TEXT("vyrn.dll") : TEXT("libvyrn.dylib");
	const FString Platform = PLATFORM_WINDOWS ? TEXT("Win64") : TEXT("Mac");
	TArray<FString> Candidates;
	if (TSharedPtr<IPlugin> Plugin = IPluginManager::Get().FindPlugin(TEXT("VyrnScripting")))
	{
		Candidates.Add(FPaths::Combine(Plugin->GetBaseDir(), TEXT("Binaries"), Platform, Name));
		Candidates.Add(FPaths::Combine(Plugin->GetBaseDir(), TEXT("Source/ThirdParty/Vyrn"), Platform, Name));
	}
	Candidates.Add(Name);
	for (const FString& Path : Candidates)
	{
		if (Path.Contains(TEXT("/")) && !FPaths::FileExists(Path))
		{
			continue;
		}
		LibraryHandle = FPlatformProcess::GetDllHandle(*Path);
		if (LibraryHandle)
		{
			break;
		}
	}
	GVyrnLoaded = LibraryHandle != nullptr;
#else
	// Linux links libvyrn.so directly.
	GVyrnLoaded = true;
#endif

	if (GVyrnLoaded)
	{
		UE_LOG(LogVyrn, Log, TEXT("Vyrn %s is ready"), UTF8_TO_TCHAR(vyrn_version()));
	}
	else
	{
		UE_LOG(LogVyrn, Error,
			TEXT("Could not load the Vyrn runtime library. Build it with "
				 "`cargo build --release --manifest-path runtime/embed/Cargo.toml` and copy it "
				 "into Plugins/VyrnScripting/Source/ThirdParty/Vyrn (see the plugin's README)."));
	}
}

void FVyrnScriptingModule::ShutdownModule()
{
	if (LibraryHandle)
	{
		FPlatformProcess::FreeDllHandle(LibraryHandle);
		LibraryHandle = nullptr;
	}
	GVyrnLoaded = false;
}

bool FVyrnScriptingModule::IsRuntimeLoaded()
{
	return GVyrnLoaded;
}

IMPLEMENT_MODULE(FVyrnScriptingModule, VyrnScripting)
