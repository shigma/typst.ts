export const enum SemanticTokensProviderStylingConstants {
  NO_STYLING = 0b01111111111111111111111111111111,
}

/**
 * https://github.com/microsoft/vscode/blob/main/src/vs/editor/common/services/semanticTokensProviderStyling.ts#128
 */
const enum SemanticColoringConstants {
  /**
   * Let's aim at having 8KB buffers if possible...
   * So that would be 8192 / (5 * 4) = 409.6 tokens per area
   */
  DesiredTokensPerArea = 400,

  /**
   * Try to keep the total number of areas under 1024 if possible,
   * simply compensate by having more tokens per area...
   */
  DesiredMaxAreas = 1024,
}

//#region Semantic tokens: https://github.com/microsoft/vscode/issues/86415
export interface SemanticTokensLegend {
  readonly tokenTypes: string[];
  readonly tokenModifiers: string[];
}

export interface SemanticTokens {
  /**
   * The result id of the tokens.
   *
   * This is the id that will be passed to `DocumentSemanticTokensProvider.provideDocumentSemanticTokensEdits` (if implemented).
   */
  readonly resultId?: string;
  readonly data: Uint32Array;
}
//#endregion

interface SemanticTokensProviderStyling {
  getMetadata(tokenTypeIndex: number, tokenModifierSet: number, languageId: string): number;
  warnInvalidLengthSemanticTokens(lineNumber: number, startCharacter: number): void;
  warnOverlappingSemanticTokens(lineNumber: number, startCharacter: number): void;
}

type SparseMultilineTokens = [number, Uint32Array];

/**
 * https://github.com/microsoft/vscode/blob/main/src/vs/editor/common/services/semanticTokensProviderStyling.ts#L142
 */
export function toMultilineTokens2(
  tokens: SemanticTokens,
  styling: SemanticTokensProviderStyling,
  languageId: string,
): SparseMultilineTokens[] {
  const srcData = tokens.data;
  const tokenCount = (tokens.data.length / 5) | 0;
  const tokensPerArea = Math.max(
    Math.ceil(tokenCount / SemanticColoringConstants.DesiredMaxAreas),
    SemanticColoringConstants.DesiredTokensPerArea,
  );
  const result: SparseMultilineTokens[] = [];

  let tokenIndex = 0;
  let lastLineNumber = 1;
  let lastStartCharacter = 0;
  while (tokenIndex < tokenCount) {
    const tokenStartIndex = tokenIndex;
    let tokenEndIndex = Math.min(tokenStartIndex + tokensPerArea, tokenCount);

    // Keep tokens on the same line in the same area...
    if (tokenEndIndex < tokenCount) {
      let smallTokenEndIndex = tokenEndIndex;
      while (smallTokenEndIndex - 1 > tokenStartIndex && srcData[5 * smallTokenEndIndex] === 0) {
        smallTokenEndIndex--;
      }

      if (smallTokenEndIndex - 1 === tokenStartIndex) {
        // there are so many tokens on this line that our area would be empty, we must now go right
        let bigTokenEndIndex = tokenEndIndex;
        while (bigTokenEndIndex + 1 < tokenCount && srcData[5 * bigTokenEndIndex] === 0) {
          bigTokenEndIndex++;
        }
        tokenEndIndex = bigTokenEndIndex;
      } else {
        tokenEndIndex = smallTokenEndIndex;
      }
    }

    let destData = new Uint32Array((tokenEndIndex - tokenStartIndex) * 4);
    let destOffset = 0;
    let areaLine = 0;
    let prevLineNumber = 0;
    let prevEndCharacter = 0;
    while (tokenIndex < tokenEndIndex) {
      const srcOffset = 5 * tokenIndex;
      const deltaLine = srcData[srcOffset];
      const deltaCharacter = srcData[srcOffset + 1];
      // Casting both `lineNumber`, `startCharacter` and `endCharacter` here to uint32 using `|0`
      // to validate below with the actual values that will be inserted in the Uint32Array result
      const lineNumber = (lastLineNumber + deltaLine) | 0;
      const startCharacter =
        deltaLine === 0 ? (lastStartCharacter + deltaCharacter) | 0 : deltaCharacter;
      const length = srcData[srcOffset + 2];
      const endCharacter = (startCharacter + length) | 0;
      const tokenTypeIndex = srcData[srcOffset + 3];
      const tokenModifierSet = srcData[srcOffset + 4];

      if (endCharacter <= startCharacter) {
        // this token is invalid (most likely a negative length casted to uint32)
        styling.warnInvalidLengthSemanticTokens(lineNumber, startCharacter + 1);
      } else if (prevLineNumber === lineNumber && prevEndCharacter > startCharacter) {
        // this token overlaps with the previous token
        styling.warnOverlappingSemanticTokens(lineNumber, startCharacter + 1);
      } else {
        const metadata = styling.getMetadata(tokenTypeIndex, tokenModifierSet, languageId);

        if (metadata !== SemanticTokensProviderStylingConstants.NO_STYLING) {
          if (areaLine === 0) {
            areaLine = lineNumber;
          }
          destData[destOffset] = lineNumber - areaLine;
          destData[destOffset + 1] = startCharacter;
          destData[destOffset + 2] = endCharacter;
          destData[destOffset + 3] = metadata;
          destOffset += 4;

          prevLineNumber = lineNumber;
          prevEndCharacter = endCharacter;
        }
      }

      lastLineNumber = lineNumber;
      lastStartCharacter = startCharacter;
      tokenIndex++;
    }

    if (destOffset !== destData.length) {
      destData = destData.subarray(0, destOffset);
    }

    result.push([areaLine, destData]);
  }

  return result;
}

const LANGUAGE_ID = 0 as const;

export class TypstSemaTokenStyling implements SemanticTokensProviderStyling {
  static _legend: SemanticTokensLegend = undefined!;

  private _hashTable: HashTable;

  constructor(private enableWarnings: boolean) {
    this._hashTable = new HashTable();
  }

  resolveTokenStyle(tokenType: string, tokenModifiers: string[], languageId: string): number {
    return SemanticTokensProviderStylingConstants.NO_STYLING;
  }

  getMetadata(tokenTypeIndex: number, tokenModifierSet: number, languageId: string): number {
    // console.log(
    //   'TypstSemanticTokensProviderStyling',
    //   TypstSemaTokenStyling._legend.tokenTypes[tokenTypeIndex],
    //   tokenModifierSet,
    // );

    const entry = this._hashTable.get(tokenTypeIndex, tokenModifierSet, LANGUAGE_ID);
    let metadata: number;
    if (entry) {
      metadata = entry.metadata;
    } else {
      let tokenType = TypstSemaTokenStyling._legend.tokenTypes[tokenTypeIndex];
      const tokenModifiers: string[] = [];
      if (tokenType) {
        let modifierSet = tokenModifierSet;
        for (
          let modifierIndex = 0;
          modifierSet > 0 && modifierIndex < TypstSemaTokenStyling._legend.tokenModifiers.length;
          modifierIndex++
        ) {
          if (modifierSet & 1) {
            tokenModifiers.push(TypstSemaTokenStyling._legend.tokenModifiers[modifierIndex]);
          }
          modifierSet = modifierSet >> 1;
        }

        metadata = this.resolveTokenStyle(tokenType, tokenModifiers, languageId);
      } else {
        metadata = SemanticTokensProviderStylingConstants.NO_STYLING;
      }
      this._hashTable.add(tokenTypeIndex, tokenModifierSet, LANGUAGE_ID, metadata);
    }

    return metadata;
  }

  warnInvalidLengthSemanticTokens(lineNumber: number, startCharacter: number): void {
    if (this.enableWarnings) {
      console.warn('warnInvalidLengthSemanticTokens', lineNumber, startCharacter);
    }
  }
  warnOverlappingSemanticTokens(lineNumber: number, startCharacter: number): void {
    if (this.enableWarnings) {
      console.warn('warnOverlappingSemanticTokens', lineNumber, startCharacter);
    }
  }
}

class HashTableEntry {
  public readonly tokenTypeIndex: number;
  public readonly tokenModifierSet: number;
  public readonly languageId: number;
  public readonly metadata: number;
  public next: HashTableEntry | null;

  constructor(
    tokenTypeIndex: number,
    tokenModifierSet: number,
    languageId: number,
    metadata: number,
  ) {
    this.tokenTypeIndex = tokenTypeIndex;
    this.tokenModifierSet = tokenModifierSet;
    this.languageId = languageId;
    this.metadata = metadata;
    this.next = null;
  }
}

class HashTable {
  private static _SIZES = [
    3, 7, 13, 31, 61, 127, 251, 509, 1021, 2039, 4093, 8191, 16381, 32749, 65521, 131071, 262139,
    524287, 1048573, 2097143,
  ];

  private _elementsCount: number;
  private _currentLengthIndex: number;
  private _currentLength: number;
  private _growCount: number;
  private _elements: (HashTableEntry | null)[];

  constructor() {
    this._elementsCount = 0;
    this._currentLengthIndex = 0;
    this._currentLength = HashTable._SIZES[this._currentLengthIndex];
    this._growCount = Math.round(
      this._currentLengthIndex + 1 < HashTable._SIZES.length ? (2 / 3) * this._currentLength : 0,
    );
    this._elements = [];
    HashTable._nullOutEntries(this._elements, this._currentLength);
  }

  private static _nullOutEntries(entries: (HashTableEntry | null)[], length: number): void {
    for (let i = 0; i < length; i++) {
      entries[i] = null;
    }
  }

  private _hash2(n1: number, n2: number): number {
    return ((n1 << 5) - n1 + n2) | 0; // n1 * 31 + n2, keep as int32
  }

  private _hashFunc(tokenTypeIndex: number, tokenModifierSet: number, languageId: number): number {
    return (
      this._hash2(this._hash2(tokenTypeIndex, tokenModifierSet), languageId) % this._currentLength
    );
  }

  public get(
    tokenTypeIndex: number,
    tokenModifierSet: number,
    languageId: number,
  ): HashTableEntry | null {
    const hash = this._hashFunc(tokenTypeIndex, tokenModifierSet, languageId);

    let p = this._elements[hash];
    while (p) {
      if (
        p.tokenTypeIndex === tokenTypeIndex &&
        p.tokenModifierSet === tokenModifierSet &&
        p.languageId === languageId
      ) {
        return p;
      }
      p = p.next;
    }

    return null;
  }

  public add(
    tokenTypeIndex: number,
    tokenModifierSet: number,
    languageId: number,
    metadata: number,
  ): void {
    this._elementsCount++;
    if (this._growCount !== 0 && this._elementsCount >= this._growCount) {
      // expand!
      const oldElements = this._elements;

      this._currentLengthIndex++;
      this._currentLength = HashTable._SIZES[this._currentLengthIndex];
      this._growCount = Math.round(
        this._currentLengthIndex + 1 < HashTable._SIZES.length ? (2 / 3) * this._currentLength : 0,
      );
      this._elements = [];
      HashTable._nullOutEntries(this._elements, this._currentLength);

      for (const first of oldElements) {
        let p = first;
        while (p) {
          const oldNext = p.next;
          p.next = null;
          this._add(p);
          p = oldNext;
        }
      }
    }
    this._add(new HashTableEntry(tokenTypeIndex, tokenModifierSet, languageId, metadata));
  }

  private _add(element: HashTableEntry): void {
    const hash = this._hashFunc(
      element.tokenTypeIndex,
      element.tokenModifierSet,
      element.languageId,
    );
    element.next = this._elements[hash];
    this._elements[hash] = element;
  }
}

enum HljsScopes {
  keyword,
  built_in,
  type,
  literal,
  number,
  operator,
  comment,
  punctuation,
  property,
  string,
  regexp,
  'char.escape',
  subst,
  symbol,
  'title.function',
  'title.class',
  variable,
  'variable.language',
  'variable.constant',
  'title',
  'title.class.inherited',
  'title.function.invoke',
  params,
  doctag,
  meta,
  'meta.prompt',
  'meta keyword',
  'meta string',
  section,
  tag,
  name,
  attr,
  attribute,
  bullet,
  code,
  emphasis,
  strong,
  formula,
  link,
  quote,
  'selector-tag',
  'selector-id',
  'selector-class',
  'selector-attr',
  'selector-pseudo',
  'template-tag',
  'template-variable',
  addition,
  deletion,
  'strong.emphasis',
}

export class TypstSemaTokenHljsStyling extends TypstSemaTokenStyling {
  static scopes = Object.values(HljsScopes).filter(v => typeof v !== 'number') as string[];
  static typeToScope = new Map<string, HljsScopes | HljsScopes>([
    ['comment', HljsScopes.comment],
    ['string', HljsScopes.string],
    ['operator', HljsScopes.operator],
    ['keyword', HljsScopes.keyword],
    ['number', HljsScopes.number],
    ['function', HljsScopes['title.function']],
    ['decorator', HljsScopes['title.function']],
    ['bool', HljsScopes.literal],
    ['punctuation', HljsScopes.punctuation],
    ['escape', HljsScopes['char.escape']],
    ['link', HljsScopes.link],
    ['raw', HljsScopes.code],
    ['label', HljsScopes.variable],
    ['ref', HljsScopes.variable],
    ['heading', HljsScopes.section],
    ['marker', HljsScopes.bullet],
    // in form of \Term
    ['term', HljsScopes.emphasis],
    ['pol', HljsScopes.variable],
    // error not rendered
    // ['error', HljsScopes.punctuation],
    // text not rendered
    // ['text', HljsScopes.punctuation],
  ]);

  constructor(enableWarnings: boolean) {
    super(enableWarnings);
  }

  resolveTokenStyle(tokenType: string, tokenModifiers: string[], languageId: string): number {
    // console.log('TypstSemaTokenHljsStyling', tokenType, tokenModifiers, languageId);

    if (tokenModifiers.includes('math')) {
      if (tokenType === 'delim') {
        return HljsScopes.punctuation;
      }

      return HljsScopes.formula;
    }

    if (tokenModifiers.length > 0) {
      if (tokenModifiers.includes('strong')) {
        if (tokenModifiers.includes('emph')) {
          return HljsScopes['strong.emphasis'];
        }

        return HljsScopes.strong;
      }

      if (tokenModifiers.includes('emph')) {
        return HljsScopes.emphasis;
      }
    }

    let encoded = TypstSemaTokenHljsStyling.typeToScope.get(tokenType);
    if (encoded !== undefined) {
      return encoded;
    }

    return SemanticTokensProviderStylingConstants.NO_STYLING;
  }

  getScope(metadata: number): string | string[] {
    if (metadata === HljsScopes['strong.emphasis']) {
      return ['strong', 'emphasis'];
    }

    return TypstSemaTokenHljsStyling.scopes[metadata];
  }
}
