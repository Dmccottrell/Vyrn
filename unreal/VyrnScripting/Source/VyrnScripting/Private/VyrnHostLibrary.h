// Registers the host functions of VyrnHostLibrary.cpp on a script component.

#pragma once

#include "CoreMinimal.h"
#include "VyrnScriptComponent.h"

void VyrnRegisterWorldFunctions(UVyrnScriptComponent* Self, const TFunction<void(const TCHAR*, FVyrnHostFunction)>& Add);
