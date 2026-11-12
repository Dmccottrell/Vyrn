// JSON helpers shared by the script component and its host functions:
// values cross between Vyrn and Unreal as JSON text.

#pragma once

#include "CoreMinimal.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Policies/CondensedJsonPrintPolicy.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"

namespace VyrnJson
{
using FArgs = TArray<TSharedPtr<FJsonValue>>;

inline FString JsonText(const FString& Text)
{
	FString Out = TEXT("\"");
	for (TCHAR C : Text)
	{
		switch (C)
		{
		case TEXT('"'): Out += TEXT("\\\""); break;
		case TEXT('\\'): Out += TEXT("\\\\"); break;
		case TEXT('\n'): Out += TEXT("\\n"); break;
		case TEXT('\r'): Out += TEXT("\\r"); break;
		case TEXT('\t'): Out += TEXT("\\t"); break;
		default:
			if (C < 0x20)
			{
				Out += FString::Printf(TEXT("\\u%04x"), static_cast<int32>(C));
			}
			else
			{
				Out.AppendChar(C);
			}
		}
	}
	Out += TEXT("\"");
	return Out;
}

inline FString JsonNumber(double N)
{
	// SanitizeFloat never gives "nan"/"inf" style text JSON can't read.
	return FMath::IsFinite(N) ? FString::SanitizeFloat(N) : TEXT("null");
}

inline FString JsonBool(bool B) { return B ? TEXT("true") : TEXT("false"); }

inline FString JsonVector(const FVector& V)
{
	return FString::Printf(TEXT("{\"x\":%s,\"y\":%s,\"z\":%s}"), *JsonNumber(V.X), *JsonNumber(V.Y), *JsonNumber(V.Z));
}

inline FString JsonRotator(const FRotator& R)
{
	return FString::Printf(
		TEXT("{\"pitch\":%s,\"yaw\":%s,\"roll\":%s}"), *JsonNumber(R.Pitch), *JsonNumber(R.Yaw), *JsonNumber(R.Roll));
}

/** A JSON value back to JSON text. */
inline FString ToJson(const TSharedPtr<FJsonValue>& Value)
{
	if (!Value.IsValid() || Value->IsNull())
	{
		return TEXT("null");
	}
	FString Out;
	FArgs One = {Value};
	TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer =
		TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Out);
	FJsonSerializer::Serialize(One, Writer);
	// Written as [value]: take off the brackets.
	return Out.Mid(1, Out.Len() - 2);
}

inline bool ParseJsonList(const FString& Text, FArgs& Out)
{
	TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Text);
	return FJsonSerializer::Deserialize(Reader, Out);
}

inline double Num(const FArgs& A, int32 I, double Default = 0.0)
{
	return A.IsValidIndex(I) && A[I].IsValid() && A[I]->Type == EJson::Number ? A[I]->AsNumber() : Default;
}

inline bool Bool(const FArgs& A, int32 I, bool Default = false)
{
	return A.IsValidIndex(I) && A[I].IsValid() && A[I]->Type == EJson::Boolean ? A[I]->AsBool() : Default;
}

inline FString Str(const FArgs& A, int32 I, const FString& Default = FString())
{
	if (!A.IsValidIndex(I) || !A[I].IsValid() || A[I]->IsNull())
	{
		return Default;
	}
	return A[I]->Type == EJson::String ? A[I]->AsString() : ToJson(A[I]);
}

inline FVector Vec(const FArgs& A, int32 First)
{
	return FVector(Num(A, First), Num(A, First + 1), Num(A, First + 2));
}

inline FColor ColorNamed(const FString& Name)
{
	const FString N = Name.ToLower();
	if (N.StartsWith(TEXT("#"))) return FColor::FromHex(N);
	if (N == TEXT("red")) return FColor::Red;
	if (N == TEXT("green")) return FColor::Green;
	if (N == TEXT("blue")) return FColor::Blue;
	if (N == TEXT("yellow")) return FColor::Yellow;
	if (N == TEXT("cyan")) return FColor::Cyan;
	if (N == TEXT("orange")) return FColor::Orange;
	if (N == TEXT("purple")) return FColor::Purple;
	if (N == TEXT("black")) return FColor::Black;
	return FColor::White;
}

inline FString Utf8ToString(const char* Text) { return Text ? FString(UTF8_TO_TCHAR(Text)) : FString(); }
} // namespace VyrnJson

