// The VyrnScripting module: loads the Vyrn runtime library when the game or
// editor starts.

#pragma once

#include "Modules/ModuleManager.h"

DECLARE_LOG_CATEGORY_EXTERN(LogVyrn, Log, All);

class FVyrnScriptingModule : public IModuleInterface
{
public:
	virtual void StartupModule() override;
	virtual void ShutdownModule() override;

	/** Whether the Vyrn runtime library loaded, so scripts can run. */
	static bool IsRuntimeLoaded();

private:
	void* LibraryHandle = nullptr;
};
